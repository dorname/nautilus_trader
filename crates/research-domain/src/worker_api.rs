//! 运行产物文档（引擎侧写入、比较与查询读取的规范化内容）。
//!
//! 不含 run_id / 墙钟时间：同配置同快照的规范化结果可跨次比对（UT-S13-05）。

/// 研究运行产物文档（内容寻址 JSON）。
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RunOutcomeDoc {
    pub kind: String,
    pub snapshot_id: String,
    pub universe_id: String,
    pub mode: String,
    pub strategy: String,
    pub fills: Vec<crate::worker_api::FillDoc>,
    pub equity_curve: Vec<crate::worker_api::EquityDoc>,
    pub final_equity_cny: String,
    pub total_return: String,
}

/// 成交明细文档。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FillDoc {
    pub instrument_id: String,
    pub date: String,
    pub side: String,
    pub qty: u64,
    pub price: String,
    pub fee_cny: String,
}

/// 每日净值文档。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EquityDoc {
    pub date: String,
    pub equity_cny: String,
}

/// 工作进程上下文（预留：子进程模式的工作区与配置哈希）。
#[derive(Debug, Clone)]
pub struct WorkerContext {
    pub workspace: std::path::PathBuf,
    pub config_hash: String,
}
