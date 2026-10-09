//! Swarm 派发账本（core-06 约束 5 幂等与恢复；issue #23）。
//!
//! redb 单表 `dispatches`：dispatch_id → JSON 记录。派发写入、终态更新；
//! GUI 重启后经 [`recover_on_startup`] 把未完成派发标记为「中断（进程重启）」
//! ——诚实口径：octos 子进程已随旧进程死亡，不得假装仍在运行。
//!
//! 账本失败（损坏/锁冲突）一律降级为日志，不阻断派发主路径。
use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

/// 账本表：dispatch_id → 派发记录 JSON。
const TABLE: TableDefinition<&str, &str> = TableDefinition::new("dispatches");

/// 派发状态（账本口径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerState {
    /// 进行中。
    Active,
    /// 已完成。
    Completed,
    /// 失败。
    Failed,
    /// 中断（进程重启）：恢复时标记。
    Interrupted,
}

/// 一条派发记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchRecord {
    /// 派发 ID（幂等键：worker 任务序号 + 进程启动时间戳）。
    pub dispatch_id: String,
    /// 开始时间（Unix 毫秒）。
    pub started_ms: u128,
    /// 契约 ID 列表（指纹组成部分）。
    pub contracts: Vec<String>,
    /// 各子任务 turn_id。
    pub turn_ids: Vec<String>,
    /// 状态。
    pub state: LedgerState,
}

/// Swarm 派发账本。
pub struct SwarmLedger {
    db: Database,
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl SwarmLedger {
    /// 打开（不存在则创建）账本。
    pub fn open(path: &Path) -> Result<Self, redb::Error> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| redb::Error::Io(std::io::Error::other(e)))?;
        }
        let db = Database::create(path)?;
        // 建表（幂等）
        let tx = db.begin_write()?;
        {
            let _t = tx.open_table(TABLE)?;
        }
        tx.commit()?;
        Ok(Self { db })
    }

    /// 记录一次派发（Active）。
    pub fn record_dispatch(
        &mut self,
        dispatch_id: &str,
        contracts: Vec<String>,
        turn_ids: Vec<String>,
    ) -> Result<(), redb::Error> {
        let rec = DispatchRecord {
            dispatch_id: dispatch_id.to_string(),
            started_ms: now_ms(),
            contracts,
            turn_ids,
            state: LedgerState::Active,
        };
        self.put(&rec)
    }

    /// 终态更新（Completed/Failed/Interrupted）。
    pub fn record_terminal(
        &mut self,
        dispatch_id: &str,
        state: LedgerState,
    ) -> Result<(), redb::Error> {
        if let Some(mut rec) = self.get(dispatch_id)? {
            rec.state = state;
            self.put(&rec)?;
        }
        Ok(())
    }

    /// 读取单条记录。
    pub fn get(&self, dispatch_id: &str) -> Result<Option<DispatchRecord>, redb::Error> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(TABLE)?;
        let v = t.get(dispatch_id)?;
        match v {
            Some(g) => Ok(serde_json::from_str(g.value()).ok()),
            None => Ok(None),
        }
    }

    /// 所有未完成（Active）派发。
    pub fn unfinished(&self) -> Result<Vec<DispatchRecord>, redb::Error> {
        let tx = self.db.begin_read()?;
        let t = tx.open_table(TABLE)?;
        let mut out = Vec::new();
        for kv in t.iter()? {
            let (_, v) = kv?;
            if let Ok(rec) = serde_json::from_str::<DispatchRecord>(v.value()) {
                if rec.state == LedgerState::Active {
                    out.push(rec);
                }
            }
        }
        Ok(out)
    }

    /// 把所有未完成派发标记为「中断（进程重启）」，返回条数。
    pub fn mark_unfinished_interrupted(&mut self) -> Result<usize, redb::Error> {
        let open = self.unfinished()?;
        let n = open.len();
        for rec in open {
            self.record_terminal(&rec.dispatch_id, LedgerState::Interrupted)?;
        }
        Ok(n)
    }

    fn put(&self, rec: &DispatchRecord) -> Result<(), redb::Error> {
        let json = serde_json::to_string(rec).map_err(|e| redb::Error::Io(std::io::Error::other(e)))?;
        let tx = self.db.begin_write()?;
        {
            let mut t = tx.open_table(TABLE)?;
            t.insert(rec.dispatch_id.as_str(), json.as_str())?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// GUI 启动恢复（core-06 约束 5）：打开账本，把未完成派发标记中断，
/// 返回标记条数。账本不存在/损坏时返回 0（不阻断启动）。
pub fn recover_on_startup(path: &Path) -> usize {
    if !path.exists() {
        return 0;
    }
    match SwarmLedger::open(path) {
        Ok(mut ledger) => match ledger.mark_unfinished_interrupted() {
            Ok(n) => n,
            Err(e) => {
                eprintln!("[swarm-ledger] 恢复标记失败（忽略）：{e}");
                0
            }
        },
        Err(e) => {
            eprintln!("[swarm-ledger] 账本打开失败（忽略）：{e}");
            0
        }
    }
}
