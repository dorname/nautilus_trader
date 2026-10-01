//! 契约类型（`core-research-contracts.yaml` definitions 节，S11 范围）。
//!
//! 金额、价格和比例使用十进制字符串；股数使用非负整数；
//! trade_date 为上海交易日 YYYY-MM-DD；内容哈希为 SHA256 小写十六进制。

use serde::{Deserialize, Serialize};

use crate::{error::ResearchError, task::TaskState, time::parse_trade_date};

/// 协议版本：主版本不匹配拒绝。
pub const PROTOCOL_VERSION: &str = "1.0.0";
/// 单条协议消息上限 1MiB（架构「凭证、容量与错误」节）。
pub const MAX_MESSAGE_BYTES: usize = 1_048_576;
/// 分页上限：limit 默认 200、最大 500。
pub const PAGE_DEFAULT_LIMIT: u32 = 200;
pub const PAGE_MAX_LIMIT: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportSource {
    Tdx,
    Tickflow,
    Auxiliary,
}

impl ImportSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tdx => "tdx",
            Self::Tickflow => "tickflow",
            Self::Auxiliary => "auxiliary",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PriceBasis {
    Raw,
    Qfq,
    Hfq,
}

impl PriceBasis {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Qfq => "qfq",
            Self::Hfq => "hfq",
        }
    }
}

/// ImportData 请求（S11）。`paths` 为只读源路径（暂存数据文件）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSpec {
    pub source: ImportSource,
    pub paths: Vec<String>,
    #[serde(default)]
    pub symbols: Option<Vec<String>>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
    pub price_basis: PriceBasis,
    #[serde(default)]
    pub auxiliary_kind: Option<String>,
}

impl ImportSpec {
    /// 字段级校验：非法输入返回 INVALID_ARGUMENT 与字段路径。
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.paths.is_empty() {
            return Err(ResearchError::invalid("导入路径不能为空").with_field("paths"));
        }
        for p in &self.paths {
            if p.trim().is_empty() {
                return Err(ResearchError::invalid("导入路径不能为空字符串").with_field("paths"));
            }
        }
        if let Some(symbols) = &self.symbols {
            for s in symbols {
                if s.trim().is_empty() {
                    return Err(ResearchError::invalid("标的代码不能为空字符串").with_field("symbols"));
                }
            }
        }
        if let Some(start) = &self.start {
            parse_trade_date(start).map_err(|e| e.with_field("start"))?;
        }
        if let Some(end) = &self.end {
            parse_trade_date(end).map_err(|e| e.with_field("end"))?;
        }
        if let (Some(start), Some(end)) = (&self.start, &self.end) {
            if start > end {
                return Err(ResearchError::invalid("开始日期不能晚于结束日期").with_field("start"));
            }
        }
        Ok(())
    }
}

/// 异步入队响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRef {
    pub task_id: String,
    pub request_id: String,
    pub state: TaskState,
}

/// 任务进度：total 未知时只显示阶段，不伪造百分比。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Progress {
    pub stage: String,
    #[serde(default)]
    pub done: Option<u64>,
    #[serde(default)]
    pub total: Option<u64>,
}

/// 契约错误体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub field: Option<String>,
    pub retryable: bool,
}

/// GetTask / CancelTask 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskView {
    pub task_id: String,
    pub state: TaskState,
    pub last_seq: i64,
    #[serde(default)]
    pub progress: Option<Progress>,
    /// 成功时必需：产物（快照 manifest）哈希。
    #[serde(default)]
    pub artifact_hash: Option<String>,
    /// S11 导入成功时的快照 ID。
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub error: Option<ErrorBody>,
}

/// 快照引用（ListSnapshots 项）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRef {
    pub snapshot_id: String,
    pub manifest_hash: String,
    pub as_of: String,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotPage {
    pub items: Vec<SnapshotRef>,
    pub next_cursor: Option<String>,
}

/// 任务事件：seq 单调递增，重复 seq 忽略。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEvent {
    pub task_id: String,
    pub seq: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub timestamp: String,
}

/// 事件类型常量。
pub mod event_type {
    pub const TASK_QUEUED: &str = "TaskQueued";
    pub const TASK_STARTED: &str = "TaskStarted";
    pub const TASK_PROGRESS: &str = "TaskProgress";
    pub const SNAPSHOT_READY: &str = "SnapshotReady";
    pub const TASK_FAILED: &str = "TaskFailed";
    pub const TASK_CANCELLED: &str = "TaskCancelled";
}

/// 行情行分页结果（快照内容只读查询）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotePage {
    pub rows: Vec<crate::quotes::QuoteRow>,
    pub next_cursor: Option<String>,
    pub total: u64,
    pub manifest_hash: String,
}

/// 校验分页 limit：1..=500，默认 200。
pub fn normalize_limit(limit: Option<u32>) -> u32 {
    match limit {
        Some(l) => l.clamp(1, PAGE_MAX_LIMIT),
        None => PAGE_DEFAULT_LIMIT,
    }
}
