//! 应用协调器：类型化命令 → 单写线程 → SQLite 元数据；导入计算在独立线程。
//!
//! CPU 约束（验收红线）：协调线程用 `crossbeam::channel::select!` 阻塞等待
//! 命令与完成通知，空闲零占用；计算线程随任务生灭，取消经原子标记在行批次
//! 边界（32 行）响应，不杀其他任务。SQLite 仅协调线程写入；只读查询走独立
//! 只读连接（WAL）。日志只记录任务动作与错误码，绝不写入凭证或环境变量。

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use crossbeam::channel::{bounded, select, Receiver, Sender};
use uuid::Uuid;

use crate::{
    error::{ErrorCode, ResearchError, Result},
    hash::{hash_canonical, sha256_file},
    manifest::{quotes_capabilities, quotes_limitations, Coverage, PartitionRef, SnapshotManifest, MANIFEST_SCHEMA_VERSION},
    objects::ObjectStore,
    parquet_io,
    protocol::{
        normalize_limit, ErrorBody, ImportSpec, Progress, QuotePage, SnapshotPage, TaskRef, TaskView,
    },
    quotes::{apply_scope, parse_staging_csv, partition_name, reject_duplicate_keys, QuoteRow},
    store::{MetadataStore, TaskRow},
    task::TaskState,
    time::now_rfc3339,
};

/// 命令确认超时（契约：3 秒，超时提示查询 request_id，不盲目重建）。
const ACK_TIMEOUT: Duration = Duration::from_secs(3);
/// 取消响应的行批次边界。
const CANCEL_CHECK_ROWS: usize = 32;

/// 协调器配置；`row_delay_ms` 与 `crash_after_partition` 为测试钩子，生产为 0/false。
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub workspace: PathBuf,
    pub row_delay_ms: u64,
    pub crash_after_partition: bool,
}

impl CoordinatorConfig {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            row_delay_ms: 0,
            crash_after_partition: false,
        }
    }
}

enum Command {
    Import {
        spec: ImportSpec,
        request_id: String,
        idempotency_key: String,
        reply: Sender<Result<TaskRef>>,
    },
    Cancel {
        task_id: String,
        reply: Sender<Result<TaskView>>,
    },
    Shutdown,
}

/// 计算线程 → 协调线程的完成通知（写库只能由协调线程执行）。
enum Completion {
    Started { task_id: String },
    Progress { task_id: String, stage: String, done: u64, total: u64 },
    Succeeded { task_id: String, staged: Box<StagedImport> },
    Failed { task_id: String, error: ResearchError },
    Cancelled { task_id: String },
    /// 模拟崩溃：分区完成、提交前进程死亡（UT-S11-03 注入点）。
    Crashed { task_id: String },
}

struct StagedPartition {
    file_name: String,
    sha256: String,
    instrument_id: String,
    rows: u64,
}

struct StagedImport {
    staging_dir: PathBuf,
    partitions: Vec<StagedPartition>,
    spec: ImportSpec,
    coverage: Coverage,
}

/// 应用协调器句柄（GUI 侧持有，发送类型化消息、读取不可变视图）。
pub struct Coordinator {
    cmd_tx: Sender<Command>,
    read: MetadataStore,
    objects: ObjectStore,
    workspace: PathBuf,
    logs: Arc<Mutex<Vec<String>>>,
    hooks: Arc<ImportHooks>,
    handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

/// 导入钩子：测试可注入逐批延迟与提交前崩溃；生产恒为 0/false。
/// 使用原子量以便协调器打开后仍可调整（例如先导入好快照、再注入崩溃）。
#[derive(Debug, Default)]
pub struct ImportHooks {
    pub row_delay_ms: std::sync::atomic::AtomicU64,
    pub crash_after_partition: AtomicBool,
}

impl Coordinator {
    /// 打开工作区：建目录、开库、启动扫描（非终态→interrupted）、挂起协调线程。
    pub fn open(config: CoordinatorConfig) -> Result<Self> {
        let ws = &config.workspace;
        fs::create_dir_all(ws.join("tmp")).map_err(|e| ResearchError::invalid(format!("创建工作区失败：{e}")))?;
        let write = MetadataStore::open(&ws.join("research.db"))?;
        let interrupted = write.interrupt_nonterminal()?;
        let read = MetadataStore::open_readonly(&ws.join("research.db"))?;
        let objects = ObjectStore::new(ws)?;
        let logs = Arc::new(Mutex::new(Vec::new()));
        if interrupted > 0 {
            push_log(&logs, format!("启动扫描：{interrupted} 个非终态任务标记为 interrupted"));
        }

        let (cmd_tx, cmd_rx) = bounded::<Command>(64);
        let (done_tx, done_rx) = bounded::<Completion>(64);
        let cancels: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let hooks = Arc::new(ImportHooks {
            row_delay_ms: std::sync::atomic::AtomicU64::new(config.row_delay_ms),
            crash_after_partition: AtomicBool::new(config.crash_after_partition),
        });
        let thread_logs = Arc::clone(&logs);
        let thread_ws = config.workspace.clone();
        let thread_hooks = Arc::clone(&hooks);
        let handle = std::thread::Builder::new()
            .name("research-coordinator".to_string())
            .spawn(move || {
                coordinator_loop(write, cmd_rx, done_rx, done_tx, cancels, thread_logs, thread_ws, thread_hooks);
            })
            .map_err(|e| ResearchError::invalid(format!("启动协调线程失败：{e}")))?;

        Ok(Self {
            cmd_tx,
            read,
            objects,
            workspace: config.workspace.clone(),
            logs,
            hooks,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// ImportData：校验、幂等入队，返回 TaskRef（命令确认超时 3 秒）。
    pub fn import_data(
        &self,
        request_id: &str,
        idempotency_key: &str,
        spec: ImportSpec,
    ) -> Result<TaskRef> {
        spec.validate()?;
        let (reply_tx, reply_rx) = bounded(1);
        self.cmd_tx
            .send(Command::Import {
                spec,
                request_id: request_id.to_string(),
                idempotency_key: idempotency_key.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ResearchError::new(ErrorCode::WorkerExited, "协调线程已退出"))?;
        reply_rx.recv_timeout(ACK_TIMEOUT).map_err(|_| {
            ResearchError::busy("命令确认超时，请用 request_id 查询任务状态，不要盲目重建")
        })?
    }

    /// CancelTask：只作用于尚未完成的任务；终态返回 ALREADY_TERMINAL。
    pub fn cancel_task(&self, task_id: &str) -> Result<TaskView> {
        let (reply_tx, reply_rx) = bounded(1);
        self.cmd_tx
            .send(Command::Cancel {
                task_id: task_id.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ResearchError::new(ErrorCode::WorkerExited, "协调线程已退出"))?;
        reply_rx.recv_timeout(ACK_TIMEOUT).map_err(|_| {
            ResearchError::busy("取消确认超时，请稍后查询任务状态")
        })?
    }

    /// GetTask：返回任务持久化视图（状态、序号、进度、产物、错误）。
    pub fn get_task(&self, task_id: &str) -> Result<TaskView> {
        let row = self
            .read
            .get_task(task_id)?
            .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{task_id}")))?;
        self.task_view(&row)
    }

    /// 轮询等待任务到达终态（测试与 GUI 均基于 GetTask，不做忙等待的内部实现）。
    pub fn wait_terminal(&self, task_id: &str, timeout: Duration) -> Result<TaskView> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let view = self.get_task(task_id)?;
            if view.state.is_terminal() {
                return Ok(view);
            }
            if std::time::Instant::now() >= deadline {
                return Err(ResearchError::busy("等待任务终态超时"));
            }
            // 调用方轮询须让出 CPU：50ms 睡眠避免空转
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// ListSnapshots：只返回已提交快照（写未提交不可见由单写线程保证）。
    pub fn list_snapshots(&self, limit: Option<u32>, cursor: Option<&str>) -> Result<SnapshotPage> {
        let (items, next_cursor) = self.read.list_snapshots(normalize_limit(limit), cursor)?;
        Ok(SnapshotPage { items, next_cursor })
    }

    /// 分页读取快照行情：cursor 绑定 manifest_hash，不允许跨版本偏移。
    pub fn read_snapshot_quotes(
        &self,
        snapshot_id: &str,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<QuotePage> {
        let snapshot = self
            .read
            .get_snapshot(snapshot_id)?
            .ok_or_else(|| ResearchError::not_found(format!("快照不存在：{snapshot_id}")))?;
        let manifest_bytes = self.objects.get(&snapshot.manifest_hash)?;
        let manifest: SnapshotManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("快照清单解析失败：{e}")))?;

        let limit = normalize_limit(limit) as usize;
        let offset = match cursor {
            None => 0usize,
            Some(c) => {
                let (hash, off) = c
                    .split_once(':')
                    .ok_or_else(|| ResearchError::invalid("无效的行情分页游标").with_field("cursor"))?;
                if hash != snapshot.manifest_hash {
                    return Err(ResearchError::invalid("分页游标与快照版本不符，请重新查询")
                        .with_field("cursor"));
                }
                off.parse()
                    .map_err(|_| ResearchError::invalid("无效的行情分页游标").with_field("cursor"))?
            }
        };

        let mut rows: Vec<QuoteRow> = Vec::new();
        for part in &manifest.partitions {
            let path = self.objects.path_of(&part.sha256);
            rows.extend(parquet_io::read_quotes_partition(&path)?);
        }
        rows.sort_by(|a, b| {
            a.instrument_id
                .cmp(&b.instrument_id)
                .then(a.trade_date.cmp(&b.trade_date))
        });
        let total = rows.len() as u64;
        let page: Vec<QuoteRow> = rows.into_iter().skip(offset).take(limit).collect();
        let next_cursor = if offset + limit < total as usize {
            Some(format!("{}:{}", snapshot.manifest_hash, offset + limit))
        } else {
            None
        };
        Ok(QuotePage {
            rows: page,
            next_cursor,
            total,
            manifest_hash: snapshot.manifest_hash,
        })
    }

    /// 回收孤儿文件：未登记内容对象 + 已终态任务的暂存目录（UT-S11-03）。
    pub fn reclaim_orphans(&self) -> Result<(usize, usize)> {
        let registered = self.read.artifact_hashes()?;
        let removed_objects = self.objects.reclaim_orphans(&registered)?;
        let tmp = self.workspace.join("tmp");
        let mut removed_dirs = 0usize;
        if tmp.is_dir() {
            for entry in fs::read_dir(&tmp).map_err(|e| ResearchError::invalid(format!("遍历暂存目录失败：{e}")))? {
                let entry = entry.map_err(|e| ResearchError::invalid(format!("读取暂存条目失败：{e}")))?;
                let name = entry.file_name().to_string_lossy().to_string();
                let active = self
                    .read
                    .get_task(&name)?
                    .is_some_and(|t| !t.state.is_terminal());
                if !active {
                    fs::remove_dir_all(entry.path())
                        .map_err(|e| ResearchError::invalid(format!("清理暂存目录失败：{e}")))?;
                    removed_dirs += 1;
                }
            }
        }
        Ok((removed_objects.len(), removed_dirs))
    }

    /// 协调器脱敏日志（测试可见；绝不包含凭证、环境变量或源文件内容）。
    pub fn logs(&self) -> Vec<String> {
        self.logs.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// 导入钩子句柄（测试注入用；生产代码不应调用）。
    pub fn import_hooks(&self) -> Arc<ImportHooks> {
        Arc::clone(&self.hooks)
    }

    fn task_view(&self, row: &TaskRow) -> Result<TaskView> {
        let progress = row
            .progress_json
            .as_ref()
            .and_then(|j| serde_json::from_str::<Progress>(j).ok());
        let error = row
            .error_json
            .as_ref()
            .and_then(|j| serde_json::from_str::<ErrorBody>(j).ok());
        let snapshot_id = if row.state == TaskState::Succeeded {
            row.result_hash
                .as_ref()
                .and_then(|h| self.read.find_snapshot_by_manifest(h).ok().flatten())
        } else {
            None
        };
        Ok(TaskView {
            task_id: row.id.clone(),
            state: row.state,
            last_seq: row.last_seq,
            progress,
            artifact_hash: row.result_hash.clone(),
            snapshot_id,
            error,
        })
    }
}

impl Drop for Coordinator {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Command::Shutdown);
        if let Ok(mut guard) = self.handle.lock() {
            if let Some(h) = guard.take() {
                let _ = h.join();
            }
        }
    }
}

// ---------------------------------------------------------------- 协调线程

#[allow(clippy::too_many_arguments)]
fn coordinator_loop(
    store: MetadataStore,
    cmd_rx: Receiver<Command>,
    done_rx: Receiver<Completion>,
    done_tx: Sender<Completion>,
    cancels: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    logs: Arc<Mutex<Vec<String>>>,
    workspace: PathBuf,
    hooks: Arc<ImportHooks>,
) {
    loop {
        // 阻塞等待：空闲时线程挂起，零 CPU 空转
        select! {
            recv(cmd_rx) -> msg => match msg {
                Ok(Command::Import { spec, request_id, idempotency_key, reply }) => {
                    let out = handle_import(&store, &done_tx, &cancels, &logs, &workspace, &hooks, spec, request_id, idempotency_key);
                    let _ = reply.send(out);
                }
                Ok(Command::Cancel { task_id, reply }) => {
                    let _ = reply.send(handle_cancel(&store, &cancels, &logs, &task_view_fn(&store), &task_id));
                }
                Ok(Command::Shutdown) | Err(_) => break,
            },
            recv(done_rx) -> msg => {
                if let Ok(c) = msg {
                    handle_completion(&store, &cancels, &logs, &workspace, c);
                }
            }
        }
    }
}

type TaskViewFn<'a> = dyn Fn(&TaskRow) -> Result<TaskView> + 'a;

fn task_view_fn(store: &MetadataStore) -> impl Fn(&TaskRow) -> Result<TaskView> + '_ {
    move |row| {
        let progress = row
            .progress_json
            .as_ref()
            .and_then(|j| serde_json::from_str::<Progress>(j).ok());
        let error = row
            .error_json
            .as_ref()
            .and_then(|j| serde_json::from_str::<ErrorBody>(j).ok());
        let snapshot_id = if row.state == TaskState::Succeeded {
            row.result_hash
                .as_ref()
                .and_then(|h| store.find_snapshot_by_manifest(h).ok().flatten())
        } else {
            None
        };
        Ok(TaskView {
            task_id: row.id.clone(),
            state: row.state,
            last_seq: row.last_seq,
            progress,
            artifact_hash: row.result_hash.clone(),
            snapshot_id,
            error,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_import(
    store: &MetadataStore,
    done_tx: &Sender<Completion>,
    cancels: &Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    logs: &Arc<Mutex<Vec<String>>>,
    workspace: &Path,
    hooks: &Arc<ImportHooks>,
    spec: ImportSpec,
    request_id: String,
    idempotency_key: String,
) -> Result<TaskRef> {
    let input_hash = hash_canonical(&spec);
    // 幂等：同键同哈希返回原任务；同键不同载荷冲突
    if let Some((hash, response)) = store.find_receipt(&idempotency_key)? {
        if hash != input_hash {
            return Err(ResearchError::new(
                ErrorCode::IdempotencyConflict,
                "相同幂等键对应不同请求",
            ));
        }
        let task_ref: TaskRef = serde_json::from_str(&response.unwrap_or_default())
            .map_err(|e| ResearchError::invalid(format!("回执解析失败：{e}")))?;
        return Ok(task_ref);
    }

    let task_id = Uuid::new_v4().to_string();
    let task_ref = TaskRef {
        task_id: task_id.clone(),
        request_id: request_id.clone(),
        state: TaskState::Queued,
    };
    let row = TaskRow {
        id: task_id.clone(),
        request_id,
        idempotency_key,
        input_hash,
        kind: "import".to_string(),
        state: TaskState::Queued,
        input_json: crate::hash::canonical_json(&spec),
        last_seq: 1,
        progress_json: None,
        result_hash: None,
        error_json: None,
    };
    let response_json = serde_json::to_string(&task_ref)
        .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?;
    store.insert_task_with_receipt(&row, &response_json)?;

    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut map) = cancels.lock() {
        map.insert(task_id.clone(), Arc::clone(&flag));
    }
    let staged_spec = spec.clone();
    let thread_done = done_tx.clone();
    let thread_ws = workspace.to_path_buf();
    let thread_hooks = Arc::clone(hooks);
    let tid = task_id.clone();
    std::thread::Builder::new()
        .name(format!("research-import-{task_id}"))
        .spawn(move || run_import(tid, staged_spec, flag, thread_done, thread_ws, thread_hooks))
        .map_err(|e| ResearchError::invalid(format!("启动导入计算线程失败：{e}")))?;
    push_log(logs, format!("导入任务 {task_id} 已入队"));
    Ok(task_ref)
}

fn handle_cancel(
    store: &MetadataStore,
    cancels: &Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    logs: &Arc<Mutex<Vec<String>>>,
    view: &TaskViewFn,
    task_id: &str,
) -> Result<TaskView> {
    let row = store
        .get_task(task_id)?
        .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{task_id}")))?;
    if row.state.is_terminal() {
        return Err(ResearchError::new(
            ErrorCode::AlreadyTerminal,
            "任务已结束，不重复取消",
        ));
    }
    if let Ok(map) = cancels.lock() {
        if let Some(flag) = map.get(task_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
    match row.state {
        TaskState::Queued => {
            // 尚未开始计算：直接落 cancelled
            store.transition_task(task_id, &[TaskState::Queued], TaskState::Cancelled, None, None, None,
                crate::protocol::event_type::TASK_CANCELLED)?;
        }
        TaskState::Running => {
            store.transition_task(task_id, &[TaskState::Running], TaskState::Cancelling, None, None, None,
                crate::protocol::event_type::TASK_PROGRESS)?;
        }
        TaskState::Cancelling => {}
        _ => {}
    }
    push_log(logs, format!("任务 {task_id} 取消请求已受理"));
    let row = store
        .get_task(task_id)?
        .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{task_id}")))?;
    view(&row)
}

fn handle_completion(
    store: &MetadataStore,
    cancels: &Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    logs: &Arc<Mutex<Vec<String>>>,
    workspace: &Path,
    completion: Completion,
) {
    let done_id = match &completion {
        Completion::Started { task_id }
        | Completion::Progress { task_id, .. }
        | Completion::Succeeded { task_id, .. }
        | Completion::Failed { task_id, .. }
        | Completion::Cancelled { task_id }
        | Completion::Crashed { task_id } => task_id.clone(),
    };
    // 仅终态通知回收取消标记；Started/Progress 移除会打断运行中的取消通路
    let terminal = matches!(
        completion,
        Completion::Succeeded { .. }
            | Completion::Failed { .. }
            | Completion::Cancelled { .. }
            | Completion::Crashed { .. }
    );
    match completion {
        Completion::Started { task_id } => {
            let _ = store.transition_task(
                &task_id,
                &[TaskState::Queued],
                TaskState::Running,
                Some(&Progress { stage: "解析暂存数据".to_string(), done: None, total: None }),
                None,
                None,
                crate::protocol::event_type::TASK_STARTED,
            );
        }
        Completion::Progress { task_id, stage, done, total } => {
            let _ = store.update_task_progress(&task_id, &Progress {
                stage,
                done: Some(done),
                total: Some(total),
            });
        }
        Completion::Succeeded { task_id, staged } => {
            match commit_import(store, &task_id, &staged) {
                Ok(true) => push_log(logs, format!("任务 {task_id} 快照已提交（{} 个分区）", staged.partitions.len())),
                Ok(false) => push_log(logs, format!("任务 {task_id} 提交被丢弃：取消已先行持久化")),
                Err(e) => push_log(logs, format!("任务 {task_id} 提交失败：{e}")),
            }
            let _ = fs::remove_dir_all(&staged.staging_dir);
        }
        Completion::Failed { task_id, error } => {
            let error_json = serde_json::to_string(&ErrorBody::from(&error)).unwrap_or_default();
            let _ = store.transition_task(
                &task_id,
                &[TaskState::Queued, TaskState::Running, TaskState::Cancelling],
                TaskState::Failed,
                None,
                None,
                Some(&error_json),
                crate::protocol::event_type::TASK_FAILED,
            );
            push_log(logs, format!("任务 {task_id} 失败：{}", ErrorBody::from(&error).code));
            let _ = fs::remove_dir_all(workspace.join("tmp").join(&task_id));
        }
        Completion::Cancelled { task_id } => {
            // 取消与完成竞争：以已持久化的完成事务为准；已完成则忽略本通知
            let _ = store.transition_task(
                &task_id,
                &[TaskState::Queued, TaskState::Running, TaskState::Cancelling],
                TaskState::Cancelled,
                None,
                None,
                None,
                crate::protocol::event_type::TASK_CANCELLED,
            );
            push_log(logs, format!("任务 {task_id} 已取消，未提交暂存产物"));
            let _ = fs::remove_dir_all(workspace.join("tmp").join(&task_id));
        }
        Completion::Crashed { task_id } => {
            // 模拟崩溃：分区已写、提交未发生 → interrupted；暂存留待孤儿回收
            let error_json = serde_json::to_string(&ErrorBody {
                code: "WORKER_EXITED".to_string(),
                message: "计算进程在提交前异常退出，任务中断".to_string(),
                field: None,
                retryable: true,
            })
            .unwrap_or_default();
            let _ = store.transition_task(
                &task_id,
                &[TaskState::Queued, TaskState::Running, TaskState::Cancelling],
                TaskState::Interrupted,
                None,
                None,
                Some(&error_json),
                crate::protocol::event_type::TASK_FAILED,
            );
            push_log(logs, format!("任务 {task_id} 中断：提交前异常退出"));
        }
    }
    if terminal {
        if let Ok(mut map) = cancels.lock() {
            map.remove(&done_id);
        }
    }
}

/// 提交快照：对象登记（复用已存在内容）→ manifest → 单事务落库。
/// 返回 false：任务已被先前事务裁决为终态（取消先行），产物登记回滚。
fn commit_import(store: &MetadataStore, task_id: &str, staged: &StagedImport) -> Result<bool> {
    let objects = ObjectStore::new(&store.workspace_root())?;
    let mut artifacts: Vec<(String, String, String, u64)> = Vec::new();
    let mut partitions: Vec<PartitionRef> = Vec::new();
    for part in &staged.partitions {
        let staged_file = staged.staging_dir.join(&part.file_name);
        let hash = objects.put_file(&staged_file)?;
        if hash != part.sha256 {
            return Err(ResearchError::new(
                ErrorCode::CorruptArtifact,
                format!("分区哈希在提交时不符：{}", part.file_name),
            ));
        }
        let size = fs::metadata(&staged_file).map(|m| m.len()).unwrap_or(0);
        let rel = ObjectStore::relative_path(&hash);
        artifacts.push((hash.clone(), rel.clone(), "quotes_partition".to_string(), size));
        partitions.push(PartitionRef {
            relative_path: rel,
            sha256: hash,
            instrument_id: part.instrument_id.clone(),
            price_basis: staged.spec.price_basis,
            rows: part.rows,
        });
    }
    partitions.sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));

    let manifest = SnapshotManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        source: staged.spec.source,
        price_basis: staged.spec.price_basis,
        coverage: staged.coverage.clone(),
        partitions,
        capabilities: quotes_capabilities(),
        limitations: quotes_limitations(),
    };
    let manifest_json = crate::hash::canonical_json(&manifest);
    let manifest_hash = objects.put(manifest_json.as_bytes())?;
    artifacts.push((
        manifest_hash.clone(),
        ObjectStore::relative_path(&manifest_hash),
        "snapshot_manifest".to_string(),
        manifest_json.len() as u64,
    ));

    // 相同内容重复导入：复用既有快照，不产生新记录
    let existing = store.find_snapshot_by_manifest(&manifest_hash)?;
    let snapshot_id = existing.as_deref().map(str::to_string);
    let new_id = snapshot_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    store.commit_import(
        task_id,
        &artifacts,
        if snapshot_id.is_some() { None } else { Some(&new_id) },
        &manifest_hash,
        manifest.as_of(),
        &serde_json::to_string(&manifest.capabilities).unwrap_or_default(),
        &serde_json::to_string(&manifest.limitations).unwrap_or_default(),
    )
}

// ---------------------------------------------------------------- 导入计算线程

fn run_import(
    task_id: String,
    spec: ImportSpec,
    cancel: Arc<AtomicBool>,
    done_tx: Sender<Completion>,
    workspace: PathBuf,
    hooks: Arc<ImportHooks>,
) {
    let send = |c: Completion| {
        let _ = done_tx.send(c);
    };
    let staging_dir = workspace.join("tmp").join(&task_id);
    if let Err(e) = fs::create_dir_all(&staging_dir) {
        send(Completion::Failed {
            task_id,
            error: ResearchError::invalid(format!("创建暂存目录失败：{e}")),
        });
        return;
    }
    send(Completion::Started {
        task_id: task_id.clone(),
    });

    // 1) 解析并校验全部暂存文件
    let mut rows: Vec<QuoteRow> = Vec::new();
    for path in &spec.paths {
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::not_found(format!("暂存数据文件不可读：{path}"))
                        .with_field("paths"),
                });
                return;
            }
        };
        match parse_staging_csv(&text) {
            Ok(mut parsed) => rows.append(&mut parsed),
            Err(row_errors) => {
                let detail = row_errors
                    .iter()
                    .take(10)
                    .map(|e| format!("第 {} 行：{}", e.line, e.reason))
                    .collect::<Vec<_>>()
                    .join("；");
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid(format!(
                        "暂存数据校验失败，整批拒绝（共 {} 处）：{detail}",
                        row_errors.len()
                    ))
                    .with_field("rows"),
                });
                return;
            }
        }
    }
    rows = apply_scope(rows, &spec);
    if rows.is_empty() {
        send(Completion::Failed {
            task_id,
            error: ResearchError::invalid("按标的与日期范围过滤后没有行情行，整批拒绝"),
        });
        return;
    }
    if let Err(e) = reject_duplicate_keys(&rows) {
        send(Completion::Failed {
            task_id,
            error: e,
        });
        return;
    }

    // 2) 行批次边界响应取消（测试钩子可注入逐批延迟）
    let total = rows.len() as u64;
    for (done, chunk) in rows.chunks(CANCEL_CHECK_ROWS).enumerate() {
        if cancel.load(Ordering::SeqCst) {
            send(Completion::Cancelled { task_id });
            return;
        }
        let delay = hooks.row_delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        send(Completion::Progress {
            task_id: task_id.clone(),
            stage: "校验行情行".to_string(),
            done: ((done + 1) * chunk.len()) as u64,
            total,
        });
    }

    // 3) 按标的与口径写 Parquet 分区到私有暂存目录
    if cancel.load(Ordering::SeqCst) {
        send(Completion::Cancelled { task_id });
        return;
    }
    let mut by_instrument: std::collections::BTreeMap<String, Vec<QuoteRow>> =
        std::collections::BTreeMap::new();
    for r in rows {
        by_instrument.entry(r.instrument_id.clone()).or_default().push(r);
    }
    let mut partitions = Vec::new();
    for (instrument, instrument_rows) in &by_instrument {
        // 分区写循环同样响应取消：写盘阶段不可吞掉已发出的取消请求
        if cancel.load(Ordering::SeqCst) {
            send(Completion::Cancelled { task_id });
            return;
        }
        let file_name = partition_name(instrument, spec.price_basis);
        let path = staging_dir.join(&file_name);
        if let Err(e) = parquet_io::write_quotes_partition(&path, instrument_rows, spec.price_basis, spec.source) {
            send(Completion::Failed {
                task_id,
                error: e,
            });
            return;
        }
        let sha256 = match sha256_file(&path) {
            Ok(h) => h,
            Err(e) => {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid(format!("分区哈希计算失败：{e}")),
                });
                return;
            }
        };
        partitions.push(StagedPartition {
            file_name,
            sha256,
            instrument_id: instrument.clone(),
            rows: instrument_rows.len() as u64,
        });
    }

    // 4) 崩溃注入点（UT-S11-03）：分区完成、提交前
    if hooks.crash_after_partition.load(Ordering::SeqCst) {
        send(Completion::Crashed { task_id });
        return;
    }

    // 提交前最后响应一次取消：与取消竞争时以先持久化者为准（见 store::commit_import）
    if cancel.load(Ordering::SeqCst) {
        send(Completion::Cancelled { task_id });
        return;
    }

    let (start, end) = by_instrument
        .values()
        .flat_map(|rs| rs.iter().map(|r| r.trade_date.clone()))
        .fold((None::<String>, None::<String>), |(lo, hi), d| {
            (
                Some(lo.map_or(d.clone(), |l| l.min(d.clone()))),
                Some(hi.map_or(d.clone(), |h| h.max(d))),
            )
        });
    send(Completion::Succeeded {
        task_id,
        staged: Box::new(StagedImport {
            staging_dir,
            partitions,
            spec,
            coverage: Coverage {
                instruments: by_instrument.len() as u64,
                rows: total,
                start: start.unwrap_or_default(),
                end: end.unwrap_or_default(),
            },
        }),
    });
}

impl From<&ResearchError> for ErrorBody {
    fn from(e: &ResearchError) -> Self {
        Self {
            code: serde_json::to_value(e.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| format!("{:?}", e.code)),
            message: e.message.clone(),
            field: e.field.clone(),
            retryable: e.retryable,
        }
    }
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut l) = logs.lock() {
        l.push(format!("{} · {line}", now_rfc3339()));
    }
}
