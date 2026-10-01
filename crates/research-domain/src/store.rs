//! SQLite 元数据存储（DDL 单源引用 `logos/resources/database/core-01-research-storage.sql`）。
//!
//! - SQLite 只允许协调器写入；读查询使用独立的只读连接；
//! - busy 超时 3 秒后返回可重试错误（BUSY）；
//! - 多表写操作全部包裹在事务中；一律使用参数化查询；
//! - 启动扫描非终态任务标为 interrupted。

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::{
    error::{ErrorCode, ResearchError, Result},
    protocol::{Progress, SnapshotRef},
    task::TaskState,
    time::now_rfc3339,
};

/// 与规格一致的 DDL（单源引用，禁止在代码里另写一份表结构）。
const SPEC_DDL: &str =
    include_str!("../../../logos/resources/database/core-01-research-storage.sql");

const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_secs(3);

/// 任务行（与 DDL task 表对齐）。
#[derive(Debug, Clone)]
pub struct TaskRow {
    pub id: String,
    pub request_id: String,
    pub idempotency_key: String,
    pub input_hash: String,
    pub kind: String,
    pub state: TaskState,
    pub input_json: String,
    pub last_seq: i64,
    pub progress_json: Option<String>,
    pub result_hash: Option<String>,
    pub error_json: Option<String>,
}

/// universe 表行（不可变：创建后不更新业务内容）。
#[derive(Debug, Clone)]
pub struct UniverseRow {
    pub id: String,
    pub name: String,
    pub snapshot_id: String,
    pub as_of: String,
    pub membership: String,
    pub mode: String,
    pub rule_json: String,
    pub version_hash: String,
    pub members_hash: String,
}

pub struct MetadataStore {
    conn: Connection,
    path: PathBuf,
}

impl MetadataStore {
    /// 打开（必要时创建）写连接：WAL、外键、busy 超时；空库时应用规格 DDL。
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| map_sqlite("打开元数据库", e))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| map_sqlite("设置 WAL", e))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| map_sqlite("设置外键", e))?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(|e| map_sqlite("设置 busy 超时", e))?;
        let store = Self {
            conn,
            path: path.to_path_buf(),
        };
        store.ensure_schema()?;
        Ok(store)
    }

    /// 打开只读连接（协调器查询通道；WAL 下读取不被短写事务阻塞）。
    pub fn open_readonly(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| map_sqlite("打开只读元数据库", e))?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(|e| map_sqlite("设置 busy 超时", e))?;
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 工作区根目录（research.db 的父目录）。
    pub fn workspace_root(&self) -> PathBuf {
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// 进度更新（running 内自迁移不经状态机校验），last_seq 递增并记 TaskProgress。
    pub fn update_task_progress(&self, id: &str, progress: &Progress) -> Result<()> {
        let progress_json = serde_json::to_string(progress)
            .map_err(|e| ResearchError::invalid(format!("进度序列化失败：{e}")))?;
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| map_sqlite("开启进度事务", e))?;
        tx.execute(
            "UPDATE task SET last_seq=last_seq+1, progress_json=?1, updated_at=?2
             WHERE id=?3 AND state IN ('queued','running','cancelling')",
            params![progress_json, now_rfc3339(), id],
        )
        .map_err(|e| map_sqlite("更新任务进度", e))?;
        let seq: i64 = tx
            .query_row("SELECT last_seq FROM task WHERE id=?1", params![id], |r| r.get(0))
            .map_err(|e| map_sqlite("读取任务序号", e))?;
        self.append_audit_tx(&tx, id, crate::protocol::event_type::TASK_PROGRESS, seq)?;
        tx.commit().map_err(|e| map_sqlite("提交进度事务", e))?;
        Ok(())
    }

    fn ensure_schema(&self) -> Result<()> {
        let exists: bool = self
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='schema_version'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| map_sqlite("检查 schema", e))?
            > 0;
        if !exists {
            self.conn
                .execute_batch(SPEC_DDL)
                .map_err(|e| map_sqlite("应用规格 DDL", e))?;
            self.conn
                .execute(
                    "INSERT INTO schema_version(version, applied_at) VALUES (?1, ?2)",
                    params![SCHEMA_VERSION, now_rfc3339()],
                )
                .map_err(|e| map_sqlite("写入 schema_version", e))?;
        }
        let version: i64 = self
            .conn
            .query_row("SELECT max(version) FROM schema_version", [], |r| r.get(0))
            .map_err(|e| map_sqlite("读取 schema_version", e))?;
        if version > SCHEMA_VERSION {
            return Err(ResearchError::new(
                ErrorCode::ProtocolVersion,
                format!("元数据 schema v{version} 高于本应用支持的 v{SCHEMA_VERSION}，拒绝打开"),
            ));
        }
        Ok(())
    }

    // ---------------------------------------------------------------- 回执与入队

    /// 按幂等键查询已提交回执（command_receipt）。
    pub fn find_receipt(&self, key: &str) -> Result<Option<(String, Option<String>)>> {
        self.conn
            .query_row(
                "SELECT input_hash, response_json FROM command_receipt WHERE idempotency_key=?1",
                params![key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| map_sqlite("查询命令回执", e))
    }

    /// 同事务写入命令回执与 queued 任务（异步命令在入队事务保存 TaskRef）。
    pub fn insert_task_with_receipt(
        &self,
        task: &TaskRow,
        receipt_response_json: &str,
    ) -> Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| map_sqlite("开启入队事务", e))?;
        tx.execute(
            "INSERT INTO task(id, request_id, idempotency_key, input_hash, kind, state, input_json, last_seq, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
            params![
                task.id,
                task.request_id,
                task.idempotency_key,
                task.input_hash,
                task.kind,
                task.state.as_str(),
                task.input_json,
                task.last_seq,
                now_rfc3339(),
            ],
        )
        .map_err(|e| map_sqlite("写入任务", e))?;
        tx.execute(
            "INSERT INTO command_receipt(idempotency_key, request_id, input_hash, method, state, response_json, created_at, updated_at)
             VALUES (?1,?2,?3,'ImportData','committed',?4,?5,?5)",
            params![task.idempotency_key, task.request_id, task.input_hash, receipt_response_json, now_rfc3339()],
        )
        .map_err(|e| map_sqlite("写入命令回执", e))?;
        self.append_audit_tx(&tx, &task.id, crate::protocol::event_type::TASK_QUEUED, task.last_seq)?;
        tx.commit().map_err(|e| map_sqlite("提交入队事务", e))?;
        Ok(())
    }

    // ---------------------------------------------------------------- 任务读取

    pub fn find_task_by_idempotency(&self, key: &str) -> Result<Option<TaskRow>> {
        self.query_task("SELECT * FROM task WHERE idempotency_key=?1", params![key])
    }

    pub fn get_task(&self, id: &str) -> Result<Option<TaskRow>> {
        self.query_task("SELECT * FROM task WHERE id=?1", params![id])
    }

    fn query_task(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Option<TaskRow>> {
        self.conn
            .query_row(sql, params, |r| {
                Ok(TaskRow {
                    id: r.get("id")?,
                    request_id: r.get("request_id")?,
                    idempotency_key: r.get("idempotency_key")?,
                    input_hash: r.get("input_hash")?,
                    kind: r.get("kind")?,
                    state: TaskState::parse(&r.get::<_, String>("state")?)
                        .unwrap_or(TaskState::Interrupted),
                    input_json: r.get("input_json")?,
                    last_seq: r.get("last_seq")?,
                    progress_json: r.get("progress_json")?,
                    result_hash: r.get("result_hash")?,
                    error_json: r.get("error_json")?,
                })
            })
            .optional()
            .map_err(|e| map_sqlite("查询任务", e))
    }

    // ---------------------------------------------------------------- 状态迁移

    /// 状态迁移 + last_seq 递增 + 审计事件，单事务提交。
    /// 通过 `WHERE state IN (...)` 乐观约束实现取消/完成竞争的持久化裁决。
    pub fn transition_task(
        &self,
        id: &str,
        from: &[TaskState],
        to: TaskState,
        progress: Option<&Progress>,
        result_hash: Option<&str>,
        error_json: Option<&str>,
        audit_kind: &str,
    ) -> Result<bool> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| map_sqlite("开启迁移事务", e))?;
        let current: String = match tx
            .query_row("SELECT state FROM task WHERE id=?1", params![id], |r| r.get(0))
            .optional()
            .map_err(|e| map_sqlite("读取任务状态", e))?
        {
            Some(s) => s,
            None => return Err(ResearchError::not_found(format!("任务不存在：{id}"))),
        };
        let current_state = TaskState::parse(&current)?;
        if !from.contains(&current_state) {
            // 竞争已被先前事务裁决（例如完成先于取消落库）
            return Ok(false);
        }
        current_state.check_transition(to)?;
        let progress_json = progress
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| ResearchError::invalid(format!("进度序列化失败：{e}")))?;
        tx.execute(
            "UPDATE task SET state=?1, last_seq=last_seq+1, progress_json=?2,
             result_hash=COALESCE(?3, result_hash), error_json=COALESCE(?4, error_json), updated_at=?5
             WHERE id=?6",
            params![
                to.as_str(),
                progress_json,
                result_hash,
                error_json,
                now_rfc3339(),
                id
            ],
        )
        .map_err(|e| map_sqlite("更新任务状态", e))?;
        let seq: i64 = tx
            .query_row("SELECT last_seq FROM task WHERE id=?1", params![id], |r| r.get(0))
            .map_err(|e| map_sqlite("读取任务序号", e))?;
        self.append_audit_tx(&tx, id, audit_kind, seq)?;
        tx.commit().map_err(|e| map_sqlite("提交迁移事务", e))?;
        Ok(true)
    }

    /// 启动扫描：非终态任务标记 interrupted（不提供断点续算），返回受影响任务数。
    pub fn interrupt_nonterminal(&self) -> Result<u64> {
        let n = self
            .conn
            .execute(
                "UPDATE task SET state='interrupted', updated_at=?1
                 WHERE state IN ('queued','running','cancelling')",
                params![now_rfc3339()],
            )
            .map_err(|e| map_sqlite("标记中断任务", e))?;
        Ok(n as u64)
    }

    // ---------------------------------------------------------------- 产物与快照

    /// 登记内容对象（幂等：同哈希重复登记忽略）。
    pub fn insert_artifact(&self, hash: &str, relative_path: &str, kind: &str, size: u64) -> Result<()> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO artifact(hash, relative_path, kind, size_bytes, created_at)
                 VALUES (?1,?2,?3,?4,?5)",
                params![hash, relative_path, kind, size as i64, now_rfc3339()],
            )
            .map_err(|e| map_sqlite("登记内容对象", e))?;
        Ok(())
    }

    pub fn artifact_hashes(&self) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash FROM artifact")
            .map_err(|e| map_sqlite("准备对象查询", e))?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| map_sqlite("查询对象哈希", e))?;
        let mut set = HashSet::new();
        for h in rows {
            set.insert(h.map_err(|e| map_sqlite("读取对象哈希", e))?);
        }
        Ok(set)
    }

    pub fn artifact_count(&self) -> Result<u64> {
        self.conn
            .query_row("SELECT count(*) FROM artifact", [], |r| r.get::<_, i64>(0))
            .map(|n| n as u64)
            .map_err(|e| map_sqlite("统计对象", e))
    }

    /// 同 manifest 哈希的快照已存在则复用（重复导入不产生新记录）。
    pub fn find_snapshot_by_manifest(&self, manifest_hash: &str) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT id FROM snapshot WHERE manifest_hash=?1",
                params![manifest_hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| map_sqlite("按清单查询快照", e))
    }

    pub fn insert_snapshot(
        &self,
        id: &str,
        manifest_hash: &str,
        as_of: &str,
        capabilities_json: &str,
        limitations_json: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO snapshot(id, manifest_hash, as_of, capabilities_json, limitations_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![id, manifest_hash, as_of, capabilities_json, limitations_json, now_rfc3339()],
            )
            .map_err(|e| map_sqlite("写入快照", e))?;
        Ok(())
    }

    /// 快照、产物与任务终态同事务提交（文件必须先就绪，见协调器提交流程）。
    /// 返回 false：任务已被先前事务裁决为终态（如取消先行），本次完成不提交任何产物。
    #[allow(clippy::too_many_arguments)]
    pub fn commit_import(
        &self,
        task_id: &str,
        artifacts: &[(String, String, String, u64)],
        snapshot_id: Option<&str>,
        manifest_hash: &str,
        as_of: &str,
        capabilities_json: &str,
        limitations_json: &str,
    ) -> Result<bool> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| map_sqlite("开启提交事务", e))?;
        let state: String = tx
            .query_row("SELECT state FROM task WHERE id=?1", params![task_id], |r| r.get(0))
            .optional()
            .map_err(|e| map_sqlite("读取任务状态", e))?
            .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{task_id}")))?;
        if !matches!(state.as_str(), "running" | "cancelling") {
            // 取消/失败已先行持久化：完成竞争失败，回滚本事务
            return Ok(false);
        }
        for (hash, rel, kind, size) in artifacts {
            tx.execute(
                "INSERT OR IGNORE INTO artifact(hash, relative_path, kind, size_bytes, created_at)
                 VALUES (?1,?2,?3,?4,?5)",
                params![hash, rel, kind, *size as i64, now_rfc3339()],
            )
            .map_err(|e| map_sqlite("登记内容对象", e))?;
        }
        if let Some(sid) = snapshot_id {
            tx.execute(
                "INSERT INTO snapshot(id, manifest_hash, as_of, capabilities_json, limitations_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![sid, manifest_hash, as_of, capabilities_json, limitations_json, now_rfc3339()],
            )
            .map_err(|e| map_sqlite("写入快照", e))?;
        }
        tx.execute(
            "UPDATE task SET state='succeeded', last_seq=last_seq+1, result_hash=?1,
             progress_json=NULL, updated_at=?2 WHERE id=?3 AND state IN ('running','cancelling')",
            params![manifest_hash, now_rfc3339(), task_id],
        )
        .map_err(|e| map_sqlite("标记任务成功", e))?;
        let seq: i64 = tx
            .query_row("SELECT last_seq FROM task WHERE id=?1", params![task_id], |r| r.get(0))
            .map_err(|e| map_sqlite("读取任务序号", e))?;
        self.append_audit_tx(&tx, task_id, crate::protocol::event_type::SNAPSHOT_READY, seq)?;
        tx.commit().map_err(|e| map_sqlite("提交导入事务", e))?;
        Ok(true)
    }

    /// 通用产物提交（预览等异步任务）：登记对象 + 任务终态同事务。
    /// 返回 false：任务已被先前事务裁决为终态（取消先行），不提交任何产物。
    pub fn commit_task_result(
        &self,
        task_id: &str,
        artifacts: &[(String, String, String, u64)],
        result_hash: &str,
        audit_kind: &str,
    ) -> Result<bool> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| map_sqlite("开启提交事务", e))?;
        let state: String = tx
            .query_row("SELECT state FROM task WHERE id=?1", params![task_id], |r| r.get(0))
            .optional()
            .map_err(|e| map_sqlite("读取任务状态", e))?
            .ok_or_else(|| ResearchError::not_found(format!("任务不存在：{task_id}")))?;
        if !matches!(state.as_str(), "running" | "cancelling") {
            return Ok(false);
        }
        for (hash, rel, kind, size) in artifacts {
            tx.execute(
                "INSERT OR IGNORE INTO artifact(hash, relative_path, kind, size_bytes, created_at)
                 VALUES (?1,?2,?3,?4,?5)",
                params![hash, rel, kind, *size as i64, now_rfc3339()],
            )
            .map_err(|e| map_sqlite("登记内容对象", e))?;
        }
        tx.execute(
            "UPDATE task SET state='succeeded', last_seq=last_seq+1, result_hash=?1,
             progress_json=NULL, updated_at=?2 WHERE id=?3 AND state IN ('running','cancelling')",
            params![result_hash, now_rfc3339(), task_id],
        )
        .map_err(|e| map_sqlite("标记任务成功", e))?;
        let seq: i64 = tx
            .query_row("SELECT last_seq FROM task WHERE id=?1", params![task_id], |r| r.get(0))
            .map_err(|e| map_sqlite("读取任务序号", e))?;
        self.append_audit_tx(&tx, task_id, audit_kind, seq)?;
        tx.commit().map_err(|e| map_sqlite("提交产物事务", e))?;
        Ok(true)
    }

    // ---------------------------------------------------------------- 股票池

    /// 保存股票池 + committed 回执 + 审计，单事务（同步命令的最终提交）。
    #[allow(clippy::too_many_arguments)]
    pub fn save_universe_with_receipt(
        &self,
        universe_id: &str,
        name: &str,
        snapshot_id: &str,
        as_of: &str,
        membership: &str,
        mode: &str,
        rule_json: &str,
        version_hash: &str,
        members_hash: &str,
        idempotency_key: &str,
        request_id: &str,
        input_hash: &str,
        response_json: &str,
    ) -> Result<()> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| map_sqlite("开启保存事务", e))?;
        tx.execute(
            "INSERT INTO universe(id, name, snapshot_id, as_of, membership, mode, rule_json, version_hash, members_hash, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![universe_id, name, snapshot_id, as_of, membership, mode, rule_json, version_hash, members_hash, now_rfc3339()],
        )
        .map_err(|e| map_sqlite("写入股票池", e))?;
        tx.execute(
            "INSERT INTO command_receipt(idempotency_key, request_id, input_hash, method, state, response_json, created_at, updated_at)
             VALUES (?1,?2,?3,'SaveUniverse','committed',?4,?5,?5)",
            params![idempotency_key, request_id, input_hash, response_json, now_rfc3339()],
        )
        .map_err(|e| map_sqlite("写入命令回执", e))?;
        self.append_audit_tx(&tx, universe_id, crate::protocol::event_type::UNIVERSE_SAVED, 0)?;
        tx.commit().map_err(|e| map_sqlite("提交保存事务", e))?;
        Ok(())
    }

    /// 按 ID 读取股票池（不可变视图）。
    pub fn get_universe(&self, id: &str) -> Result<Option<UniverseRow>> {
        self.conn
            .query_row(
                "SELECT id, name, snapshot_id, as_of, membership, mode, rule_json, version_hash, members_hash FROM universe WHERE id=?1",
                params![id],
                |r| {
                    Ok(UniverseRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        snapshot_id: r.get(2)?,
                        as_of: r.get(3)?,
                        membership: r.get(4)?,
                        mode: r.get(5)?,
                        rule_json: r.get(6)?,
                        version_hash: r.get(7)?,
                        members_hash: r.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(|e| map_sqlite("查询股票池", e))
    }

    // ---------------------------------------------------------------- 快照查询

    /// 只返回已提交快照；keyset 分页（cursor 为上一页末行 rowid）。
    pub fn list_snapshots(&self, limit: u32, cursor: Option<&str>) -> Result<(Vec<SnapshotRef>, Option<String>)> {
        let after: i64 = match cursor {
            None => 0,
            Some(c) => c
                .parse()
                .map_err(|_| ResearchError::invalid("无效的快照分页游标").with_field("cursor"))?,
        };
        let mut stmt = self
            .conn
            .prepare(
                "SELECT rowid, id, manifest_hash, as_of, capabilities_json, limitations_json
                 FROM snapshot WHERE rowid > ?1 ORDER BY rowid LIMIT ?2",
            )
            .map_err(|e| map_sqlite("准备快照查询", e))?;
        let rows = stmt
            .query_map(params![after, i64::from(limit) + 1], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    SnapshotRef {
                        snapshot_id: r.get(1)?,
                        manifest_hash: r.get(2)?,
                        as_of: r.get(3)?,
                        capabilities: serde_json::from_str(&r.get::<_, String>(4)?)
                            .unwrap_or_default(),
                        limitations: serde_json::from_str(&r.get::<_, String>(5)?)
                            .unwrap_or_default(),
                    },
                ))
            })
            .map_err(|e| map_sqlite("查询快照", e))?;
        let mut items = Vec::new();
        let mut last_rowid = 0;
        for row in rows {
            let (rowid, item) = row.map_err(|e| map_sqlite("读取快照", e))?;
            last_rowid = rowid;
            items.push(item);
        }
        let next_cursor = if items.len() > limit as usize {
            items.truncate(limit as usize);
            Some(last_rowid.to_string())
        } else {
            None
        };
        Ok((items, next_cursor))
    }

    /// 按 manifest 哈希取快照（GetTask 成功视图的 snapshot_id）。
    pub fn get_snapshot(&self, snapshot_id: &str) -> Result<Option<SnapshotRef>> {
        self.conn
            .query_row(
                "SELECT id, manifest_hash, as_of, capabilities_json, limitations_json FROM snapshot WHERE id=?1",
                params![snapshot_id],
                |r| {
                    Ok(SnapshotRef {
                        snapshot_id: r.get(0)?,
                        manifest_hash: r.get(1)?,
                        as_of: r.get(2)?,
                        capabilities: serde_json::from_str(&r.get::<_, String>(3)?)
                            .unwrap_or_default(),
                        limitations: serde_json::from_str(&r.get::<_, String>(4)?)
                            .unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(|e| map_sqlite("读取快照", e))
    }

    // ---------------------------------------------------------------- 审计

    fn append_audit_tx(
        &self,
        tx: &rusqlite::Transaction,
        subject_id: &str,
        kind: &str,
        seq: i64,
    ) -> Result<()> {
        let payload = serde_json::json!({"task_id": subject_id, "seq": seq, "type": kind,
            "timestamp": now_rfc3339()});
        tx.execute(
            "INSERT INTO audit_event(subject_id, kind, payload_json, created_at) VALUES (?1,?2,?3,?4)",
            params![subject_id, kind, payload.to_string(), now_rfc3339()],
        )
        .map_err(|e| map_sqlite("写入审计事件", e))?;
        Ok(())
    }
}

fn map_sqlite(context: &str, e: rusqlite::Error) -> ResearchError {
    if let rusqlite::Error::SqliteFailure(code, _) = &e {
        if code.code == rusqlite::ErrorCode::DatabaseBusy {
            return ResearchError::busy(format!("{context}：数据库忙，可重试"));
        }
        if code.code == rusqlite::ErrorCode::DiskFull {
            return ResearchError::new(ErrorCode::DiskFull, format!("{context}：磁盘空间不足"));
        }
    }
    ResearchError::invalid(format!("{context}：{e}"))
}
