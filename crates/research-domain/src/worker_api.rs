//! 运行产物文档（引擎侧写入、比较与查询读取的规范化内容）。
//!
//! 行类型对齐契约 RowsPage.row_types（equity/holdings/fills）。
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
    pub holdings: Vec<crate::worker_api::HoldingsDoc>,
    pub final_equity_cny: String,
    pub total_return: String,
}

/// 成交明细文档（契约 fills 行类型）。
/// fill_id 规范化（F-<标的>-<日期>-<序>），reason 首版为空串（成交无拒单理由）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FillDoc {
    pub fill_id: String,
    pub instrument_id: String,
    pub trade_date: String,
    pub side: String,
    pub quantity: u64,
    pub raw_price: String,
    pub fees_cny: String,
    pub reason: String,
}

/// 每日净值文档（契约 equity 行类型）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EquityDoc {
    pub trade_date: String,
    pub cash_cny: String,
    pub positions_value_cny: String,
    pub receivables_cny: String,
    pub equity_cny: String,
}

/// 每日持仓文档（契约 holdings 行类型）。
/// sellable_quantity 为扣除当日买入（T+1）后的可卖数量。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HoldingsDoc {
    pub trade_date: String,
    pub instrument_id: String,
    pub quantity: u64,
    pub sellable_quantity: u64,
    pub mark_price: String,
    pub market_value_cny: String,
}

/// 工作进程上下文（预留：子进程模式的工作区与配置哈希）。
#[derive(Debug, Clone)]
pub struct WorkerContext {
    pub workspace: std::path::PathBuf,
    pub config_hash: String,
}
