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
    auxiliary::{
        financial_from_bytes, financial_json_bytes, master_from_bytes, master_json_bytes,
        parse_financial_csv, parse_master_csv, sniff_kind, AuxKind, FinancialRecord, MasterRecord,
    },
    error::{ErrorCode, ResearchError, Result},
    hash::{hash_canonical, sha256_file},
    manifest::{capabilities, limitations, AuxRef, Coverage, PartitionRef, SnapshotManifest, MANIFEST_SCHEMA_VERSION},
    objects::ObjectStore,
    parquet_io,
    protocol::{
        event_type, normalize_limit, ErrorBody, ImportSpec, Progress, QuotePage, RowsPage,
        RowsTable, SaveUniverseSpec, SnapshotPage, TaskRef, TaskView, UniversePreview, UniverseRef,
        UniverseSpec,
    },
    quotes::{apply_scope, parse_staging_csv, partition_name, reject_duplicate_keys, QuoteRow, STAGING_CSV_HEADER},
    store::{MetadataStore, TaskRow},
    task::TaskState,
    time::now_rfc3339,
    universe::{self, MemberRow, Tri},
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
    /// 研究运行专属工作进程二进制（SubmitRun 必须；凭证不传入子进程环境）。
    pub worker_bin: Option<PathBuf>,
}

impl CoordinatorConfig {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            row_delay_ms: 0,
            crash_after_partition: false,
            worker_bin: None,
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
    PreviewUniverse {
        spec: UniverseSpec,
        request_id: String,
        idempotency_key: String,
        reply: Sender<Result<TaskRef>>,
    },
    SaveUniverse {
        spec: SaveUniverseSpec,
        request_id: String,
        idempotency_key: String,
        reply: Sender<Result<UniverseRef>>,
    },
    SubmitRun {
        spec: crate::protocol::RunSpec,
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
pub enum Completion {
    Started { task_id: String },
    Progress { task_id: String, stage: String, done: u64, total: u64 },
    Succeeded { task_id: String, staged: Box<StagedImport> },
    /// 预览完成：产物已写入对象存储，由协调线程登记并落终态。
    PreviewSucceeded { task_id: String, artifact_hash: String, size: u64 },
    /// 运行子进程退出：exit_code = Some(0) 正常（读产物裁决终态），None 为被信号终止。
    RunExited { task_id: String, exit_code: Option<i32> },
    Failed { task_id: String, error: ResearchError },
    Cancelled { task_id: String },
    /// 模拟崩溃：分区完成、提交前进程死亡（UT-S11-03 注入点）。
    Crashed { task_id: String },
}

pub struct StagedPartition {
    pub file_name: String,
    pub sha256: String,
    pub instrument_id: String,
    pub rows: u64,
}

/// 暂存的辅助数据内容对象（JSON 字节已写暂存目录并算好哈希）。
pub struct StagedAux {
    pub kind: AuxKind,
    pub file_name: String,
    pub sha256: String,
    pub rows: u64,
}

pub struct StagedImport {
    pub staging_dir: PathBuf,
    pub partitions: Vec<StagedPartition>,
    pub auxiliary: Vec<StagedAux>,
    pub spec: ImportSpec,
    pub coverage: Coverage,
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
        let thread_worker_bin = config.worker_bin.clone();
        let handle = std::thread::Builder::new()
            .name("research-coordinator".to_string())
            .spawn(move || {
                coordinator_loop(write, cmd_rx, done_rx, done_tx, cancels, thread_logs, thread_ws, thread_hooks, thread_worker_bin);
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

    /// PreviewUniverse：校验 + 严格预检 + 幂等入队（S12）。
    pub fn preview_universe(
        &self,
        request_id: &str,
        idempotency_key: &str,
        spec: UniverseSpec,
    ) -> Result<TaskRef> {
        spec.validate()?;
        let (reply_tx, reply_rx) = bounded(1);
        self.cmd_tx
            .send(Command::PreviewUniverse {
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

    /// SaveUniverse：同步命令，哈希必须与当前预览一致（STALE_PREVIEW）。
    pub fn save_universe(
        &self,
        request_id: &str,
        idempotency_key: &str,
        spec: SaveUniverseSpec,
    ) -> Result<UniverseRef> {
        spec.validate()?;
        let (reply_tx, reply_rx) = bounded(1);
        self.cmd_tx
            .send(Command::SaveUniverse {
                spec,
                request_id: request_id.to_string(),
                idempotency_key: idempotency_key.to_string(),
                reply: reply_tx,
            })
            .map_err(|_| ResearchError::new(ErrorCode::WorkerExited, "协调线程已退出"))?;
        reply_rx.recv_timeout(ACK_TIMEOUT).map_err(|_| {
            ResearchError::busy("命令确认超时，请用 request_id 查询，不要盲目重建")
        })?
    }

    /// 预览摘要（三态计数与哈希），供保存前核对。
    pub fn universe_preview(&self, preview_task_id: &str) -> Result<UniversePreview> {
        let row = self
            .read
            .get_task(preview_task_id)?
            .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{preview_task_id}")))?;
        if row.kind != "universe_preview" {
            return Err(ResearchError::invalid("任务不是股票池预览").with_field("preview_task_id"));
        }
        if row.state != TaskState::Succeeded {
            return Err(ResearchError::new(ErrorCode::RunNotReady, "预览任务尚未成功"));
        }
        let hash = row
            .result_hash
            .clone()
            .ok_or_else(|| ResearchError::new(ErrorCode::CorruptArtifact, "预览任务缺少产物哈希"))?;
        let doc = self.read_preview_doc(&hash)?;
        let spec: UniverseSpec = serde_json::from_str(&row.input_json)
            .map_err(|e| ResearchError::invalid(format!("预览输入解析失败：{e}")))?;
        Ok(UniversePreview {
            preview_hash: hash,
            input_hash: row.input_hash.clone(),
            pass: doc.rows.iter().filter(|r| r.verdict == "pass").count() as u64,
            exclude: doc.rows.iter().filter(|r| r.verdict == "exclude").count() as u64,
            unknown: doc.rows.iter().filter(|r| r.verdict == "unknown").count() as u64,
            rows_object_id: row.id.clone(),
            snapshot_id: spec.snapshot_id,
            as_of: spec.as_of,
        })
    }

    /// 股票池不可变视图（ST-S12-02 断言原池不变用）。
    pub fn get_universe(&self, universe_id: &str) -> Result<Option<crate::store::UniverseRow>> {
        self.read.get_universe(universe_id)
    }

    /// QueryRows：按对象（预览任务/已存股票池）分页读取成员/排除/未知。
    pub fn query_rows(
        &self,
        object_id: &str,
        table: RowsTable,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<RowsPage> {
        let object_hash = self.resolve_rows_object(object_id)?;
        let doc = self.read_preview_doc(&object_hash)?;
        let limit = normalize_limit(limit) as usize;
        let offset = match cursor {
            None => 0usize,
            Some(c) => {
                let (hash, off) = c
                    .split_once(':')
                    .ok_or_else(|| ResearchError::invalid("无效的分页游标").with_field("cursor"))?;
                if hash != object_hash {
                    return Err(ResearchError::invalid("分页游标与对象版本不符，请重新查询")
                        .with_field("cursor"));
                }
                off.parse()
                    .map_err(|_| ResearchError::invalid("无效的分页游标").with_field("cursor"))?
            }
        };
        let filtered: Vec<MemberRow> = doc
            .rows
            .into_iter()
            .filter(|r| r.verdict == table.verdict())
            .collect();
        let total = filtered.len() as u64;
        let page: Vec<MemberRow> = filtered.into_iter().skip(offset).take(limit).collect();
        let next_cursor = if offset + limit < total as usize {
            Some(format!("{object_hash}:{}", offset + limit))
        } else {
            None
        };
        Ok(RowsPage { rows: page, next_cursor, object_hash, total })
    }

    /// 解析 QueryRows 对象：预览任务 ID 或已存股票池 ID。
    fn resolve_rows_object(&self, object_id: &str) -> Result<String> {
        if let Some(task) = self.read.get_task(object_id)? {
            if task.kind == "universe_preview" && task.state == TaskState::Succeeded {
                return task.result_hash.clone().ok_or_else(|| {
                    ResearchError::new(ErrorCode::CorruptArtifact, "预览任务缺少产物哈希")
                });
            }
            return Err(ResearchError::new(ErrorCode::RunNotReady, "预览任务尚未成功"));
        }
        if let Some(u) = self.read.get_universe(object_id)? {
            return Ok(u.members_hash);
        }
        Err(ResearchError::not_found(format!("查询对象不存在：{object_id}")))
    }

    fn read_preview_doc(&self, hash: &str) -> Result<PreviewDoc> {
        let bytes = self.objects.get(hash)?;
        serde_json::from_slice(&bytes).map_err(|e| {
            ResearchError::new(ErrorCode::CorruptArtifact, format!("预览产物解析失败：{e}"))
        })
    }

    /// SubmitRun：预检（UT-S13-06）→ 入队（网格拆父子任务）→ 专属子进程执行。
    /// 凭证策略：子进程环境只传工作区路径，绝不继承 TICKFLOW_API_KEY。
    pub fn submit_run(
        &self,
        request_id: &str,
        idempotency_key: &str,
        spec: crate::protocol::RunSpec,
    ) -> Result<TaskRef> {
        spec.validate()?;
        let (reply_tx, reply_rx) = bounded(1);
        self.cmd_tx
            .send(Command::SubmitRun {
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

    /// GetTask：返回任务持久化视图（状态、序号、进度、产物、错误、网格父子视图）。
    pub fn get_task(&self, task_id: &str) -> Result<TaskView> {
        let row = self
            .read
            .get_task(task_id)?
            .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{task_id}")))?;
        let mut view = self.task_view(&row)?;
        view.parent_id = row.parent_id.clone();
        if row.kind == "research_run_grid" {
            view.children = self
                .read
                .list_children(&row.id)?
                .iter()
                .map(|c| crate::protocol::ChildTaskView {
                    task_id: c.id.clone(),
                    state: c.state,
                    result_hash: c.result_hash.clone(),
                    config_hash: Some(c.input_hash.clone()),
                })
                .collect();
        }
        Ok(view)
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
            children: Vec::new(),
            parent_id: row.parent_id.clone(),
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
    worker_bin: Option<PathBuf>,
) {
    loop {
        // 阻塞等待：空闲时线程挂起，零 CPU 空转
        select! {
            recv(cmd_rx) -> msg => match msg {
                Ok(Command::Import { spec, request_id, idempotency_key, reply }) => {
                    let out = handle_import(&store, &done_tx, &cancels, &logs, &workspace, &hooks, spec, request_id, idempotency_key);
                    let _ = reply.send(out);
                }
                Ok(Command::PreviewUniverse { spec, request_id, idempotency_key, reply }) => {
                    let out = handle_preview(&store, &done_tx, &cancels, &logs, &workspace, &hooks, spec, request_id, idempotency_key);
                    let _ = reply.send(out);
                }
                Ok(Command::SaveUniverse { spec, request_id, idempotency_key, reply }) => {
                    let out = handle_save_universe(&store, &logs, &workspace, spec, request_id, idempotency_key);
                    let _ = reply.send(out);
                }
                Ok(Command::SubmitRun { spec, request_id, idempotency_key, reply }) => {
                    let out = handle_submit_run(&store, &done_tx, &cancels, &logs, &workspace, worker_bin.as_deref(), spec, request_id, idempotency_key);
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
            children: Vec::new(),
            parent_id: row.parent_id.clone(),
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
        parent_id: None,
        retry_of: None,
    };
    let response_json = serde_json::to_string(&task_ref)
        .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?;
    store.insert_task_with_receipt(&row, &response_json, "ImportData")?;

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

/// 预览产物文档（内容寻址 JSON；行按标的排序，哈希确定性）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreviewDoc {
    pub kind: String,
    pub as_of: String,
    pub snapshot_id: String,
    pub mode: String,
    pub membership: String,
    pub rule: universe::RuleGroup,
    pub counts: PreviewCounts,
    pub rows: Vec<MemberRow>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreviewCounts {
    pub pass: u64,
    pub exclude: u64,
    pub unknown: u64,
}

/// PreviewUniverse 入队：快照存在性 + 严格预检（缺能力即拒）+ 幂等。
#[allow(clippy::too_many_arguments)]
fn handle_preview(
    store: &MetadataStore,
    done_tx: &Sender<Completion>,
    cancels: &Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    logs: &Arc<Mutex<Vec<String>>>,
    workspace: &Path,
    hooks: &Arc<ImportHooks>,
    spec: UniverseSpec,
    request_id: String,
    idempotency_key: String,
) -> Result<TaskRef> {
    let input_hash = hash_canonical(&spec);
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

    // 快照与严格预检（协调线程内同步完成，返回前已裁决）
    let snapshot = store
        .get_snapshot(&spec.snapshot_id)?
        .ok_or_else(|| ResearchError::not_found(format!("快照不存在：{}", spec.snapshot_id)))?;
    let objects = ObjectStore::new(&store.workspace_root())?;
    let manifest_bytes = objects.get(&snapshot.manifest_hash)?;
    let manifest: SnapshotManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("快照清单解析失败：{e}")))?;
    precheck_universe(&spec, &manifest)?;

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
        kind: "universe_preview".to_string(),
        state: TaskState::Queued,
        input_json: crate::hash::canonical_json(&spec),
        last_seq: 1,
        progress_json: None,
        result_hash: None,
        error_json: None,
        parent_id: None,
        retry_of: None,
    };
    let response_json = serde_json::to_string(&task_ref)
        .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?;
    store.insert_task_with_receipt(&row, &response_json, "PreviewUniverse")?;

    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut map) = cancels.lock() {
        map.insert(task_id.clone(), Arc::clone(&flag));
    }
    let thread_done = done_tx.clone();
    let thread_ws = workspace.to_path_buf();
    let thread_hooks = Arc::clone(hooks);
    let tid = task_id.clone();
    std::thread::Builder::new()
        .name(format!("research-preview-{task_id}"))
        .spawn(move || run_preview(tid, spec, flag, thread_done, thread_ws, thread_hooks))
        .map_err(|e| ResearchError::invalid(format!("启动预览计算线程失败：{e}")))?;
    push_log(logs, format!("预览任务 {task_id} 已入队"));
    Ok(task_ref)
}

/// 严格/探索预检：规则引用能力必须在快照能力内；价格规则受覆盖区间约束。
fn precheck_universe(spec: &UniverseSpec, manifest: &SnapshotManifest) -> Result<()> {
    // 收集规则引用的能力
    let mut needed = std::collections::BTreeSet::new();
    for child in &spec.rule.children {
        match child {
            universe::RuleChild::Cond(c) => {
                needed.insert(c.field.capability());
            }
            universe::RuleChild::Or(g) => {
                for c in &g.children {
                    needed.insert(c.field.capability());
                }
            }
        }
    }
    for cap in &needed {
        if !manifest.capabilities.iter().any(|c| c == cap) {
            if spec.mode == crate::protocol::UniverseMode::Strict {
                return Err(ResearchError::new(
                    ErrorCode::MissingCapability,
                    format!("严格预检拒绝：快照缺少能力 {cap}（请先导入对应辅助数据）"),
                ));
            }
        }
    }
    // 价格类规则的 as_of 不得晚于快照覆盖末日（不能用未来数据）
    if needed.contains("quotes.daily") && !manifest.as_of().is_empty() && spec.as_of.as_str() > manifest.as_of() {
        return Err(ResearchError::invalid(format!(
            "as_of {} 晚于快照覆盖末日 {}，无法按时点评估",
            spec.as_of,
            manifest.as_of()
        ))
        .with_field("as_of"));
    }
    Ok(())
}

/// SaveUniverse（同步）：哈希一致性校验 + 不可变保存 + 回执，单事务。
fn handle_save_universe(
    store: &MetadataStore,
    logs: &Arc<Mutex<Vec<String>>>,
    workspace: &Path,
    spec: SaveUniverseSpec,
    request_id: String,
    idempotency_key: String,
) -> Result<UniverseRef> {
    let input_hash = hash_canonical(&spec);
    if let Some((hash, response)) = store.find_receipt(&idempotency_key)? {
        if hash != input_hash {
            return Err(ResearchError::new(
                ErrorCode::IdempotencyConflict,
                "相同幂等键对应不同请求",
            ));
        }
        let saved: UniverseRef = serde_json::from_str(&response.unwrap_or_default())
            .map_err(|e| ResearchError::invalid(format!("回执解析失败：{e}")))?;
        return Ok(saved);
    }

    let task = store
        .get_task(&spec.preview_task_id)?
        .ok_or_else(|| ResearchError::not_found(format!("预览任务不存在：{}", spec.preview_task_id)))?;
    if task.kind != "universe_preview" {
        return Err(ResearchError::invalid("preview_task_id 不是预览任务").with_field("preview_task_id"));
    }
    if task.state != TaskState::Succeeded {
        return Err(ResearchError::new(ErrorCode::RunNotReady, "预览任务尚未成功，不能保存"));
    }
    // 陈旧预览：规则修改后保存旧预览 → STALE_PREVIEW（ST-S12-02）
    let result_hash = task.result_hash.clone().unwrap_or_default();
    if spec.input_hash != task.input_hash || spec.preview_hash != result_hash {
        return Err(ResearchError::new(
            ErrorCode::StalePreview,
            "输入已改变，请重新预览后再保存",
        ));
    }

    let universe_spec: UniverseSpec = serde_json::from_str(&task.input_json)
        .map_err(|e| ResearchError::invalid(format!("预览输入解析失败：{e}")))?;
    let objects = ObjectStore::new(workspace)?;
    let doc_bytes = objects.get(&result_hash)?;
    let doc: PreviewDoc = serde_json::from_slice(&doc_bytes)
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("预览产物解析失败：{e}")))?;

    let rule_hash = hash_canonical(&universe_spec.rule);
    #[derive(serde::Serialize)]
    struct VersionContent<'a> {
        rule_hash: &'a str,
        members_hash: &'a str,
        snapshot_id: &'a str,
        as_of: &'a str,
        membership: &'a str,
        mode: &'a str,
    }
    let version_hash = hash_canonical(&VersionContent {
        rule_hash: &rule_hash,
        members_hash: &result_hash,
        snapshot_id: &universe_spec.snapshot_id,
        as_of: &universe_spec.as_of,
        membership: universe_spec.membership.as_str(),
        mode: universe_spec.mode.as_str(),
    });

    let universe_id = Uuid::new_v4().to_string();
    let universe_ref = UniverseRef {
        universe_id: universe_id.clone(),
        version_hash,
        rule_hash,
        snapshot_id: universe_spec.snapshot_id.clone(),
        as_of: universe_spec.as_of.clone(),
        count: doc.counts.pass,
    };
    let response_json = serde_json::to_string(&universe_ref)
        .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?;
    store.save_universe_with_receipt(
        &universe_id,
        &spec.name,
        &universe_spec.snapshot_id,
        &universe_spec.as_of,
        universe_spec.membership.as_str(),
        universe_spec.mode.as_str(),
        &crate::hash::canonical_json(&universe_spec.rule),
        &universe_ref.version_hash,
        &result_hash,
        &idempotency_key,
        &request_id,
        &input_hash,
        &response_json,
    )?;
    push_log(logs, format!("股票池 {universe_id} 已保存（{} 名成员）", doc.counts.pass));
    Ok(universe_ref)
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
        | Completion::PreviewSucceeded { task_id, .. }
        | Completion::RunExited { task_id, .. }
        | Completion::Failed { task_id, .. }
        | Completion::Cancelled { task_id }
        | Completion::Crashed { task_id } => task_id.clone(),
    };
    // 仅终态通知回收取消标记；Started/Progress 移除会打断运行中的取消通路
    let terminal = matches!(
        completion,
        Completion::Succeeded { .. }
            | Completion::PreviewSucceeded { .. }
            | Completion::Failed { .. }
            | Completion::Cancelled { .. }
            | Completion::Crashed { .. }
            | Completion::RunExited { .. }
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
        Completion::PreviewSucceeded { task_id, artifact_hash, size } => {
            let rel = ObjectStore::relative_path(&artifact_hash);
            let artifacts = vec![(artifact_hash.clone(), rel, "universe_preview".to_string(), size)];
            match store.commit_task_result(&task_id, &artifacts, &artifact_hash, event_type::PREVIEW_READY) {
                Ok(true) => push_log(logs, format!("预览任务 {task_id} 产物已提交")),
                Ok(false) => push_log(logs, format!("预览任务 {task_id} 提交被丢弃：取消已先行持久化")),
                Err(e) => push_log(logs, format!("预览任务 {task_id} 提交失败：{e}")),
            }
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
        Completion::RunExited { task_id, exit_code } => {
            handle_run_exit(store, logs, workspace, &task_id, exit_code);
        }
    }
    if terminal {
        if let Ok(mut map) = cancels.lock() {
            map.remove(&done_id);
        }
    }
}

/// 运行子进程正常退出（exit 0）：读产物对象裁决终态并回填 research_run；
/// 网格父任务在全部子任务终态后聚合（首项结果保留，失败/取消不掩盖已完成项）。
fn handle_run_exit(
    store: &MetadataStore,
    logs: &Arc<Mutex<Vec<String>>>,
    workspace: &Path,
    task_id: &str,
    exit_code: Option<i32>,
) {
    debug_assert_eq!(exit_code, Some(0), "RunExited 仅承载正常退出");
    // 工作器清单：tmp/{task_id}/result.json = 产物哈希（元数据库只由协调器写）
    let manifest_file = workspace.join("tmp").join(task_id).join("result.json");
    let result_hash = match fs::read_to_string(&manifest_file) {
        Ok(h) if h.len() == 64 => h,
        _ => {
            push_log(logs, format!("运行任务 {task_id} 无有效结果清单，按失败处理"));
            return;
        }
    };
    let size = ObjectStore::new(workspace)
        .map(|o| fs::metadata(o.path_of(&result_hash)).map(|m| m.len()).unwrap_or(0))
        .unwrap_or(0);
    let artifacts = vec![(
        result_hash.clone(),
        ObjectStore::relative_path(&result_hash),
        "research_run".to_string(),
        size,
    )];
    match store.commit_task_result(task_id, &artifacts, &result_hash, event_type::RUN_COMPLETED) {
        Ok(true) => {
            let _ = store.complete_research_run_audit(task_id);
            push_log(logs, format!("运行任务 {task_id} 结果已提交"));
        }
        Ok(false) => push_log(logs, format!("运行任务 {task_id} 提交被丢弃：取消已先行持久化")),
        Err(e) => push_log(logs, format!("运行任务 {task_id} 提交失败：{e}")),
    }
    let _ = fs::remove_dir_all(workspace.join("tmp").join(task_id));
    resolve_grid_parent(store, logs, task_id);
}

/// 网格父任务聚合：全部子任务终态后，父任务 succeeded（部分失败仍算父任务完成，
/// 子任务各自状态可见——不得把整个网格标全部成功，也不得掩盖已完成项）。
fn resolve_grid_parent(store: &MetadataStore, logs: &Arc<Mutex<Vec<String>>>, child_id: &str) {
    let Some(row) = store.get_task(child_id).ok().flatten() else {
        return;
    };
    let Some(parent_id) = row.parent_id else { return };
    let children = store.list_children(&parent_id).unwrap_or_default();
    if children.iter().any(|c| !c.state.is_terminal()) {
        return;
    }
    // 父任务终态迁移：若有任一子任务 succeeded → 父 succeeded；全失败/取消 → failed
    // 状态机要求经 Running 中转（Queued→Succeeded 非法），先推进再聚合
    let _ = store.transition_task(
        &parent_id,
        &[TaskState::Queued],
        TaskState::Running,
        None,
        None,
        None,
        event_type::TASK_STARTED,
    );
    let any_succeeded = children.iter().any(|c| c.state == TaskState::Succeeded);
    let to = if any_succeeded { TaskState::Succeeded } else { TaskState::Failed };
    // DDL 约束：succeeded 必须带 result_hash——父任务产物是子运行清单（哈希→状态）
    let parent_hash = if to == TaskState::Succeeded {
        let listing: std::collections::BTreeMap<&String, String> = children
            .iter()
            .map(|c| {
                (
                    &c.id,
                    c.result_hash.clone().unwrap_or_else(|| c.state.as_str().to_string()),
                )
            })
            .collect();
        match ObjectStore::new(&store.workspace_root())
            .and_then(|o| {
                let h = o.put(crate::hash::canonical_json(&listing).as_bytes())?;
                let size = fs::metadata(o.path_of(&h)).map(|m| m.len()).unwrap_or(0);
                store.insert_artifact(&h, &ObjectStore::relative_path(&h), "research_run_grid", size)?;
                Ok(h)
            })
        {
            Ok(h) => Some(h),
            Err(e) => {
                push_log(logs, format!("网格父任务 {parent_id} 清单产物失败：{e}"));
                None
            }
        }
    } else {
        None
    };
    let moved = store.transition_task(
        &parent_id,
        &[TaskState::Running],
        to,
        None,
        parent_hash.as_deref(),
        None,
        if any_succeeded { event_type::GRID_RESOLVED } else { event_type::TASK_FAILED },
    );
    if let Err(e) = moved {
        push_log(logs, format!("网格父任务 {parent_id} 聚合迁移失败：{e}"));
    } else {
        push_log(logs, format!("网格父任务 {parent_id} 聚合完成：{to:?}"));
    }
    let _ = moved;
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

    // 辅助数据内容对象：登记并进入 manifest（master/financial）
    let mut aux_refs: Vec<AuxRef> = Vec::new();
    for aux in &staged.auxiliary {
        let staged_file = staged.staging_dir.join(&aux.file_name);
        let hash = objects.put_file(&staged_file)?;
        if hash != aux.sha256 {
            return Err(ResearchError::new(
                ErrorCode::CorruptArtifact,
                format!("辅助数据哈希在提交时不符：{}", aux.file_name),
            ));
        }
        let size = fs::metadata(&staged_file).map(|m| m.len()).unwrap_or(0);
        let rel = ObjectStore::relative_path(&hash);
        artifacts.push((hash.clone(), rel.clone(), format!("aux_{}", aux.kind.as_str()), size));
        aux_refs.push(AuxRef { kind: aux.kind.as_str().to_string(), relative_path: rel, sha256: hash, rows: aux.rows });
    }
    aux_refs.sort_by(|a, b| a.kind.cmp(&b.kind));
    let aux_kinds: Vec<String> = aux_refs.iter().map(|a| a.kind.clone()).collect();
    let aux_kind_refs: Vec<&str> = aux_kinds.iter().map(String::as_str).collect();

    let manifest = SnapshotManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        source: staged.spec.source,
        price_basis: staged.spec.price_basis,
        coverage: staged.coverage.clone(),
        partitions,
        auxiliary: aux_refs,
        capabilities: capabilities(!staged.partitions.is_empty(), &aux_kind_refs),
        limitations: limitations(&aux_kind_refs),
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

    // 1) 解析并校验全部暂存文件（按表头识别行情/主档/财务；整批拒绝）
    let declared_aux = spec.auxiliary_kind.as_deref().and_then(AuxKind::parse);
    let mut rows: Vec<QuoteRow> = Vec::new();
    let mut master_rows: Vec<MasterRecord> = Vec::new();
    let mut financial_rows: Vec<FinancialRecord> = Vec::new();
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
        let fail_rows = |task_id: String, errors: Vec<crate::quotes::RowError>| {
            let detail = errors
                .iter()
                .take(10)
                .map(|e| format!("第 {} 行：{}", e.line, e.reason))
                .collect::<Vec<_>>()
                .join("；");
            send(Completion::Failed {
                task_id,
                error: ResearchError::invalid(format!(
                    "暂存数据校验失败，整批拒绝（共 {} 处）：{detail}",
                    errors.len()
                ))
                .with_field("rows"),
            });
        };
        let header = text.lines().next().unwrap_or("").trim().trim_start_matches('\u{feff}');
        if header == STAGING_CSV_HEADER {
            if declared_aux.is_some() {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid("声明 auxiliary_kind 的导入不允许混入行情文件")
                        .with_field("paths"),
                });
                return;
            }
            match parse_staging_csv(&text) {
                Ok(mut parsed) => rows.append(&mut parsed),
                Err(e) => {
                    fail_rows(task_id, e);
                    return;
                }
            }
        } else {
            let kind = match sniff_kind(&text) {
                Some(k) => k,
                None => {
                    send(Completion::Failed {
                        task_id,
                        error: ResearchError::invalid(format!(
                            "无法识别暂存文件种类（表头不符）：{path}"
                        ))
                        .with_field("paths"),
                    });
                    return;
                }
            };
            if declared_aux.is_some_and(|k| k != kind) {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid(format!(
                        "文件种类 {} 与声明的 auxiliary_kind 不符",
                        kind.as_str()
                    ))
                    .with_field("auxiliary_kind"),
                });
                return;
            }
            match kind {
                AuxKind::Master => match parse_master_csv(&text) {
                    Ok(mut parsed) => master_rows.append(&mut parsed),
                    Err(e) => {
                        fail_rows(task_id, e);
                        return;
                    }
                },
                AuxKind::Financial => match parse_financial_csv(&text) {
                    Ok(mut parsed) => financial_rows.append(&mut parsed),
                    Err(e) => {
                        fail_rows(task_id, e);
                        return;
                    }
                },
            }
        }
    }
    rows = apply_scope(rows, &spec);
    // symbols 过滤同样作用于辅助数据
    if let Some(symbols) = &spec.symbols {
        master_rows.retain(|r| symbols.contains(&r.instrument_id));
        financial_rows.retain(|r| symbols.contains(&r.instrument_id));
    }
    if rows.is_empty() && master_rows.is_empty() && financial_rows.is_empty() {
        send(Completion::Failed {
            task_id,
            error: ResearchError::invalid("按标的与日期范围过滤后没有数据行，整批拒绝"),
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
    // 跨文件主键冲突检查（文件内已查，此处查合并后）
    {
        let mut seen = std::collections::HashSet::new();
        for r in &master_rows {
            if !seen.insert(r.instrument_id.as_str()) {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid(format!("主档标的重复：{}", r.instrument_id))
                        .with_field("rows"),
                });
                return;
            }
        }
        let mut seen = std::collections::HashSet::new();
        for r in &financial_rows {
            if !seen.insert((r.instrument_id.as_str(), r.period_end.as_str(), r.revision)) {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid(format!(
                        "财务记录重复：{} {} 修订号 {}",
                        r.instrument_id, r.period_end, r.revision
                    ))
                    .with_field("rows"),
                });
                return;
            }
        }
    }

    // 2) 行批次边界响应取消（测试钩子可注入逐批延迟）
    let total = (rows.len() + master_rows.len() + financial_rows.len()) as u64;
    let quote_total = rows.len() as u64;
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
            stage: "校验数据行".to_string(),
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

    // 3b) 辅助数据写规范化 JSON 内容对象到暂存目录（确定性字节 → 确定性哈希）
    let mut staged_aux: Vec<StagedAux> = Vec::new();
    for (kind, bytes, n) in [
        (AuxKind::Master, master_json_bytes(&master_rows), master_rows.len()),
        (AuxKind::Financial, financial_json_bytes(&financial_rows), financial_rows.len()),
    ] {
        if n == 0 {
            continue;
        }
        if cancel.load(Ordering::SeqCst) {
            send(Completion::Cancelled { task_id });
            return;
        }
        let file_name = format!("aux-{}.json", kind.as_str());
        let path = staging_dir.join(&file_name);
        if let Err(e) = fs::write(&path, &bytes) {
            send(Completion::Failed {
                task_id,
                error: ResearchError::invalid(format!("写入辅助数据暂存失败：{e}")),
            });
            return;
        }
        let sha256 = match sha256_file(&path) {
            Ok(h) => h,
            Err(e) => {
                send(Completion::Failed {
                    task_id,
                    error: ResearchError::invalid(format!("辅助数据哈希计算失败：{e}")),
                });
                return;
            }
        };
        staged_aux.push(StagedAux { kind, file_name, sha256, rows: n as u64 });
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
    let instruments = {
        let mut set = std::collections::BTreeSet::new();
        set.extend(by_instrument.keys().cloned());
        set.extend(master_rows.iter().map(|r| r.instrument_id.clone()));
        set.extend(financial_rows.iter().map(|r| r.instrument_id.clone()));
        set.len() as u64
    };
    send(Completion::Succeeded {
        task_id,
        staged: Box::new(StagedImport {
            staging_dir,
            partitions,
            auxiliary: staged_aux,
            spec,
            coverage: Coverage {
                instruments,
                rows: quote_total,
                start: start.unwrap_or_default(),
                end: end.unwrap_or_default(),
            },
        }),
    });
}

// ---------------------------------------------------------------- 预览计算线程

fn run_preview(
    task_id: String,
    spec: UniverseSpec,
    cancel: Arc<AtomicBool>,
    done_tx: Sender<Completion>,
    workspace: PathBuf,
    hooks: Arc<ImportHooks>,
) {
    let send = |c: Completion| {
        let _ = done_tx.send(c);
    };
    let fail = |task_id: String, error: ResearchError| send(Completion::Failed { task_id, error });
    send(Completion::Started {
        task_id: task_id.clone(),
    });

    // 加载快照内容（只读连接 + 对象存储）
    let store = match MetadataStore::open_readonly(&workspace.join("research.db")) {
        Ok(s) => s,
        Err(e) => return fail(task_id, e),
    };
    let objects = match ObjectStore::new(&workspace) {
        Ok(o) => o,
        Err(e) => return fail(task_id, e),
    };
    let snapshot = match store.get_snapshot(&spec.snapshot_id) {
        Ok(Some(s)) => s,
        Ok(None) => return fail(task_id, ResearchError::not_found(format!("快照不存在：{}", spec.snapshot_id))),
        Err(e) => return fail(task_id, e),
    };
    let manifest: SnapshotManifest = match objects.get(&snapshot.manifest_hash)
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| {
            ResearchError::new(ErrorCode::CorruptArtifact, format!("快照清单解析失败：{e}"))
        })) {
        Ok(m) => m,
        Err(e) => return fail(task_id, e),
    };

    // 主档 / 财务 / 行情 装载（缺能力时为 None，条件评估为未知）
    let load_aux = |kind: &str| -> Result<Option<Vec<u8>>> {
        match manifest.aux(kind) {
            Some(a) => objects.get(&a.sha256).map(Some),
            None => Ok(None),
        }
    };
    let master: Option<std::collections::BTreeMap<String, MasterRecord>> = match load_aux("master") {
        Ok(Some(b)) => match master_from_bytes(&b) {
            Ok(rows) => Some(rows.into_iter().map(|r| (r.instrument_id.clone(), r)).collect()),
            Err(e) => return fail(task_id, e),
        },
        Ok(None) => None,
        Err(e) => return fail(task_id, e),
    };
    let financial: Option<std::collections::BTreeMap<String, Vec<FinancialRecord>>> = match load_aux("financial") {
        Ok(Some(b)) => match financial_from_bytes(&b) {
            Ok(rows) => {
                let mut map: std::collections::BTreeMap<String, Vec<FinancialRecord>> = Default::default();
                for r in rows {
                    map.entry(r.instrument_id.clone()).or_default().push(r);
                }
                Some(map)
            }
            Err(e) => return fail(task_id, e),
        },
        Ok(None) => None,
        Err(e) => return fail(task_id, e),
    };
    let quotes: Option<std::collections::BTreeMap<String, Vec<QuoteRow>>> = if manifest.partitions.is_empty() {
        None
    } else {
        let mut map: std::collections::BTreeMap<String, Vec<QuoteRow>> = Default::default();
        for part in &manifest.partitions {
            let path = objects.path_of(&part.sha256);
            match parquet_io::read_quotes_partition(&path) {
                Ok(rows) => map.insert(part.instrument_id.clone(), rows),
                Err(e) => return fail(task_id, e),
            };
        }
        Some(map)
    };

    let ctx = universe::EvalContext {
        as_of: &spec.as_of,
        master: master.as_ref(),
        financial: financial.as_ref(),
        quotes: quotes.as_ref(),
        ignore_unknown_conditions: spec.missing_policy == Some(crate::protocol::MissingPolicy::IgnoreCondition),
    };
    let candidates = universe::candidates(&ctx);
    let total = candidates.len() as u64;

    let mut rows: Vec<MemberRow> = Vec::with_capacity(candidates.len());
    for (idx, instrument) in candidates.iter().enumerate() {
        // 批次边界响应取消（32 标的）；测试钩子注入逐批延迟
        if idx % CANCEL_CHECK_ROWS == 0 {
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
                stage: "评估筛选规则".to_string(),
                done: idx as u64,
                total,
            });
        }
        let (verdict, reasons, field_values) = universe::eval_group(&spec.rule, instrument, &ctx);
        rows.push(MemberRow {
            instrument_id: instrument.clone(),
            name: String::new(),
            as_of: spec.as_of.clone(),
            verdict: match verdict {
                Tri::Hit => "pass",
                Tri::Miss => "exclude",
                Tri::Unknown => "unknown",
            }
            .to_string(),
            reasons,
            field_values,
            available_at: None,
        });
    }

    // 提交前最后响应一次取消
    if cancel.load(Ordering::SeqCst) {
        send(Completion::Cancelled { task_id });
        return;
    }

    let doc = PreviewDoc {
        kind: "universe_preview".to_string(),
        as_of: spec.as_of.clone(),
        snapshot_id: spec.snapshot_id.clone(),
        mode: spec.mode.as_str().to_string(),
        membership: spec.membership.as_str().to_string(),
        rule: spec.rule.clone(),
        counts: PreviewCounts {
            pass: rows.iter().filter(|r| r.verdict == "pass").count() as u64,
            exclude: rows.iter().filter(|r| r.verdict == "exclude").count() as u64,
            unknown: rows.iter().filter(|r| r.verdict == "unknown").count() as u64,
        },
        rows,
    };
    let bytes = crate::hash::canonical_json(&doc).into_bytes();
    let size = bytes.len() as u64;
    let artifact_hash = match objects.put(&bytes) {
        Ok(h) => h,
        Err(e) => return fail(task_id, e),
    };
    send(Completion::PreviewSucceeded { task_id, artifact_hash, size });
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

// ---------------------------------------------------------------- S13 运行编排（批次 3c）

/// 单个子运行的确定性配置文档（写入对象存储，工作进程读取后执行）。
/// 内容不含 run_id 与墙钟 → 同配置同快照哈希一致（确定性，UT-S13-05）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunConfigDoc {
    pub kind: String,
    pub workspace: String,
    pub snapshot_id: String,
    pub universe_id: String,
    pub spec: crate::protocol::RunSpec,
    /// 网格覆盖项（如 fast=5）；单次运行为空。
    #[serde(default)]
    pub grid_override: std::collections::BTreeMap<String, String>,
}

/// 网格展开：按 BTreeMap 键序笛卡尔积（确定性顺序）。
fn expand_grid(
    grid: &std::collections::BTreeMap<String, Vec<String>>,
) -> Vec<std::collections::BTreeMap<String, String>> {
    let mut acc: Vec<std::collections::BTreeMap<String, String>> = vec![std::collections::BTreeMap::new()];
    for (name, values) in grid {
        let mut next = Vec::new();
        for base in &acc {
            for v in values {
                let mut m = base.clone();
                m.insert(name.clone(), v.clone());
                next.push(m);
            }
        }
        acc = next;
    }
    acc
}

/// SubmitRun：幂等 → 存在性预检（快照/股票池）→ 入队（网格拆父子）→ 起子进程守望。
#[allow(clippy::too_many_arguments)]
fn handle_submit_run(
    store: &MetadataStore,
    done_tx: &Sender<Completion>,
    cancels: &Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    logs: &Arc<Mutex<Vec<String>>>,
    workspace: &Path,
    worker_bin: Option<&Path>,
    spec: crate::protocol::RunSpec,
    request_id: String,
    idempotency_key: String,
) -> Result<TaskRef> {
    let input_hash = hash_canonical(&spec);
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

    // 预检失败不启动任何引擎（规格：预检失败不启动引擎）
    let snapshot = store
        .get_snapshot(&spec.snapshot_id)?
        .ok_or_else(|| ResearchError::not_found(format!("快照不存在：{}", spec.snapshot_id)))?;
    let universe = store
        .get_universe(&spec.universe_id)?
        .ok_or_else(|| ResearchError::not_found(format!("股票池不存在：{}", spec.universe_id)))?;
    if universe.snapshot_id != spec.snapshot_id {
        return Err(ResearchError::invalid(format!(
            "股票池 {} 来自快照 {}，与运行快照 {} 不一致",
            spec.universe_id, universe.snapshot_id, spec.snapshot_id
        ))
        .with_field("universe_id"));
    }
    let _ = snapshot;

    // 网格展开（无 grid → 单覆盖空表 = 单任务，不建父）
    let overrides = expand_grid(spec.grid.as_ref().unwrap_or(&Default::default()));
    let single = spec.grid.is_none();
    let parent_id = if single { None } else { Some(Uuid::new_v4().to_string()) };

    // 父任务（网格）
    if let Some(pid) = &parent_id {
        let parent_row = TaskRow {
            id: pid.clone(),
            request_id: format!("{request_id}:parent"),
            idempotency_key: format!("{idempotency_key}:parent"),
            input_hash: input_hash.clone(),
            kind: "research_run_grid".to_string(),
            state: TaskState::Queued,
            input_json: crate::hash::canonical_json(&spec),
            last_seq: 1,
            progress_json: None,
            result_hash: None,
            error_json: None,
            parent_id: None,
            retry_of: None,
        };
        store.insert_task_with_receipt(
            &parent_row,
            &serde_json::to_string(&TaskRef {
                task_id: pid.clone(),
                request_id: request_id.clone(),
                state: TaskState::Queued,
            })
            .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?,
            "SubmitRun",
        )?;
    }

    // 响应 TaskRef：单次=子任务本身；网格=父任务（children 经 GetTask 查询）
    let task_id = parent_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
    let task_ref = TaskRef {
        task_id: task_id.clone(),
        request_id: request_id.clone(),
        state: TaskState::Queued,
    };

    // 子任务 + 运行配置文档 + research_run 行
    let mut child_ids = Vec::new();
    for grid_override in &overrides {
        let child_id = if single { task_id.clone() } else { Uuid::new_v4().to_string() };
        let doc = RunConfigDoc {
            kind: "research_run_config".to_string(),
            workspace: workspace.to_string_lossy().to_string(),
            snapshot_id: spec.snapshot_id.clone(),
            universe_id: spec.universe_id.clone(),
            spec: spec.clone(),
            grid_override: grid_override.clone(),
        };
        let doc_bytes = crate::hash::canonical_json(&doc).into_bytes();
        let objects = ObjectStore::new(workspace)?;
        let doc_hash = objects.put(&doc_bytes)?;
        let config_hash = crate::hash::hash_canonical(&doc);
        // 任务 input_hash = 配置哈希（任务层语义）；幂等键绑定的请求哈希在外层校验，
        // single 任务的幂等键直接复用请求键，故 task.input_hash 在 single 时取请求哈希
        let row = TaskRow {
            id: child_id.clone(),
            request_id: if single {
                request_id.clone()
            } else {
                format!("{request_id}:child:{child_id}")
            },
            idempotency_key: if single {
                idempotency_key.clone()
            } else {
                format!("{idempotency_key}:child:{config_hash}")
            },
            input_hash: if single { input_hash.clone() } else { config_hash.clone() },
            kind: "research_run".to_string(),
            state: TaskState::Queued,
            input_json: String::from_utf8(doc_bytes.clone())
                .map_err(|e| ResearchError::invalid(format!("配置文档编码失败：{e}")))?,
            last_seq: 1,
            progress_json: None,
            result_hash: None,
            error_json: None,
            parent_id: parent_id.clone(),
            retry_of: None,
        };
        store.insert_task_with_receipt(
            &row,
            &serde_json::to_string(&TaskRef {
                task_id: child_id.clone(),
                request_id: request_id.clone(),
                state: TaskState::Queued,
            })
            .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?,
            "SubmitRun",
        )?;
        store.insert_research_run(
            &child_id,
            &spec.snapshot_id,
            &spec.universe_id,
            &String::from_utf8(doc_bytes).map_err(|e| ResearchError::invalid(format!("配置文档编码失败：{e}")))?,
            &config_hash,
            env!("CARGO_PKG_VERSION"),
        )?;
        child_ids.push((child_id, doc_hash, config_hash));
    }

    // 幂等响应以"响应 TaskRef"为准写入回执（网格=父任务）
    store.write_receipt_response(
        &idempotency_key,
        &serde_json::to_string(&task_ref)
            .map_err(|e| ResearchError::invalid(format!("回执序列化失败：{e}")))?,
    )?;

    // 每个子任务起专属守望线程：spawn 子进程 → 轮询取消标记/退出 → 完成通知
    let flag_for_children: Vec<Arc<AtomicBool>> = child_ids
        .iter()
        .map(|(cid, _, _)| {
            let flag = Arc::new(AtomicBool::new(false));
            if let Ok(mut map) = cancels.lock() {
                map.insert(cid.clone(), Arc::clone(&flag));
            }
            flag
        })
        .collect();
    for ((child_id, doc_hash, _), flag) in child_ids.iter().zip(flag_for_children) {
        let thread_done = done_tx.clone();
        let thread_logs = Arc::clone(logs);
        let tid = child_id.clone();
        let hash = doc_hash.clone();
        let bin = worker_bin.map(Path::to_path_buf);
        let ws = workspace.to_path_buf();
        std::thread::Builder::new()
            .name(format!("research-run-{child_id}"))
            .spawn(move || {
                run_run_process(tid, hash, bin, ws, flag, thread_done, thread_logs);
            })
            .map_err(|e| ResearchError::invalid(format!("启动运行守望线程失败：{e}")))?;
    }
    push_log(
        logs,
        format!("研究运行已入队：{}（{} 个子运行）", task_id, overrides.len()),
    );
    Ok(task_ref)
}

/// 运行守望线程：spawn 专属工作进程（凭证不入环境）→ 转发进度 → 退出后通知。
/// 取消：标记置位后 kill 子进程（UT-S13-07 的 5 秒上限由 wait_timeout 兜底）。
fn run_run_process(
    task_id: String,
    config_hash: String,
    worker_bin: Option<PathBuf>,
    workspace: PathBuf,
    cancel: Arc<AtomicBool>,
    done: Sender<Completion>,
    logs: Arc<Mutex<Vec<String>>>,
) {
    let send = |c: Completion| {
        let _ = done.send(c);
    };
    let _ = done.send(Completion::Started { task_id: task_id.clone() });

    // 未配置 worker_bin：使用注册的进程内执行器（引擎与协调器同进程，测试路径；
    // 生产应配置 worker_bin 以获得进程隔离）。执行器返回退出码语义与子进程一致。
    let bin = match worker_bin {
        Some(b) => b,
        None => {
            let exit = crate::executor::run_in_process(&workspace, &config_hash, &cancel, &task_id, &done);
            match exit {
                crate::executor::EXIT_OK => {
                    push_log(&logs, format!("运行任务 {task_id} 执行完成"));
                    send(Completion::RunExited { task_id, exit_code: Some(0) });
                }
                crate::executor::EXIT_CANCELLED => {
                    push_log(&logs, format!("运行任务 {task_id} 已取消"));
                    send(Completion::Cancelled { task_id });
                }
                crate::executor::EXIT_CRASHED => {
                    push_log(&logs, format!("运行任务 {task_id} 崩溃（提交前）"));
                    send(Completion::Crashed { task_id });
                }
                _ => {
                    push_log(&logs, format!("运行任务 {task_id} 失败（exit={exit}）"));
                    send(Completion::Failed {
                        error: ResearchError::new(
                            ErrorCode::WorkerExited,
                            format!("工作进程退出码 {exit}"),
                        ),
                        task_id,
                    });
                }
            }
            return;
        }
    };

    let mut cmd = std::process::Command::new(&bin);
    // 凭证红线：不继承父进程环境，只传工作区与配置哈希
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(["run-task", &workspace.to_string_lossy(), &config_hash]);
    let child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            send(Completion::Failed {
                error: ResearchError::new(
                    ErrorCode::WorkerExited,
                    format!("启动工作进程失败：{e}"),
                ),
                task_id,
            });
            return;
        }
    };
    // 50ms 轮询取消标记与退出：进程等待无法 select，50ms 粒度足够交易日边界
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            push_log(&logs, format!("运行任务 {task_id} 工作进程已终止（取消）"));
            send(Completion::Cancelled { task_id });
            return;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code();
                push_log(&logs, format!("运行任务 {task_id} 工作进程退出：{status}"));
                send(Completion::RunExited { task_id, exit_code: code });
                return;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                send(Completion::Failed {
                    error: ResearchError::new(
                        ErrorCode::WorkerExited,
                        format!("工作进程等待失败：{e}"),
                    ),
                    task_id,
                });
                return;
            }
        }
    }
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, line: String) {
    if let Ok(mut l) = logs.lock() {
        l.push(format!("{} · {line}", now_rfc3339()));
    }
}
