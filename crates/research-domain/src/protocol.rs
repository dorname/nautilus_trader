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
        if let Some(k) = &self.auxiliary_kind {
            if crate::auxiliary::AuxKind::parse(k).is_none() {
                return Err(ResearchError::invalid(format!(
                    "未知辅助数据种类：{k}（支持 master/financial）"
                ))
                .with_field("auxiliary_kind"));
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
    pub const UNIVERSE_SAVED: &str = "UniverseSaved";
    pub const PREVIEW_READY: &str = "UniversePreviewReady";
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

// ---------------------------------------------------------------- S12 股票池

/// 股票池模式：严格（缺能力预检拒绝）/ 探索（缺数据记未知）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UniverseMode {
    Strict,
    Exploratory,
}

impl UniverseMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Exploratory => "exploratory",
        }
    }
}

/// 成员口径：动态（保存规则版本，回测逐日重算）/ 固定（保存形成日成员）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Membership {
    Dynamic,
    Fixed,
}

impl Membership {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dynamic => "dynamic",
            Self::Fixed => "fixed",
        }
    }
}

/// 缺失数据处理：exclude（默认，计入未知桶）/ ignore_condition（仅探索模式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingPolicy {
    Exclude,
    IgnoreCondition,
}

/// PreviewUniverse 请求（S12）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseSpec {
    pub snapshot_id: String,
    pub as_of: String,
    pub mode: UniverseMode,
    pub membership: Membership,
    pub rule: crate::universe::RuleGroup,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_policy: Option<MissingPolicy>,
}

impl UniverseSpec {
    /// 字段级校验 + 规则静态校验 + 模式约束。
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.snapshot_id.trim().is_empty() {
            return Err(ResearchError::invalid("snapshot_id 不能为空").with_field("snapshot_id"));
        }
        parse_trade_date(&self.as_of).map_err(|e| e.with_field("as_of"))?;
        crate::universe::validate_rule(&self.rule)?;
        if self.missing_policy == Some(MissingPolicy::IgnoreCondition)
            && self.mode != UniverseMode::Exploratory
        {
            return Err(ResearchError::invalid(
                "missing_policy=ignore_condition 只能用于探索模式",
            )
            .with_field("missing_policy"));
        }
        Ok(())
    }
}

/// SaveUniverse 请求（S12）：哈希必须与当前预览一致，否则 STALE_PREVIEW。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveUniverseSpec {
    pub preview_task_id: String,
    pub preview_hash: String,
    pub input_hash: String,
    pub name: String,
}

impl SaveUniverseSpec {
    pub fn validate(&self) -> crate::error::Result<()> {
        if self.preview_task_id.trim().is_empty() {
            return Err(ResearchError::invalid("preview_task_id 不能为空").with_field("preview_task_id"));
        }
        if self.preview_hash.len() != 64 {
            return Err(ResearchError::invalid("preview_hash 必须是 SHA256 十六进制").with_field("preview_hash"));
        }
        if self.input_hash.len() != 64 {
            return Err(ResearchError::invalid("input_hash 必须是 SHA256 十六进制").with_field("input_hash"));
        }
        let name_len = self.name.chars().count();
        if !(1..=80).contains(&name_len) {
            return Err(ResearchError::invalid("名称长度须为 1..80 字符").with_field("name"));
        }
        Ok(())
    }
}

/// SaveUniverse 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniverseRef {
    pub universe_id: String,
    pub version_hash: String,
    pub rule_hash: String,
    pub snapshot_id: String,
    pub as_of: String,
    pub count: u64,
}

/// 预览产物摘要（UniversePreview，任务成功时可通过 GetTask 间接核对哈希）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversePreview {
    pub preview_hash: String,
    pub input_hash: String,
    pub pass: u64,
    pub exclude: u64,
    pub unknown: u64,
    pub rows_object_id: String,
    pub snapshot_id: String,
    pub as_of: String,
}

/// QueryRows 表名（S12 本批实现 members/excluded/unknown）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowsTable {
    Members,
    Excluded,
    Unknown,
}

impl RowsTable {
    /// 对应的预览行判定值。
    pub fn verdict(&self) -> &'static str {
        match self {
            Self::Members => "pass",
            Self::Excluded => "exclude",
            Self::Unknown => "unknown",
        }
    }
}

/// QueryRows 响应：cursor 绑定对象哈希，跨版本偏移拒绝。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowsPage {
    pub rows: Vec<crate::universe::MemberRow>,
    pub next_cursor: Option<String>,
    pub object_hash: String,
    pub total: u64,
}
