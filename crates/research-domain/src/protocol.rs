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
    /// 网格父任务的子任务视图；非父任务为空数组。
    #[serde(default)]
    pub children: Vec<ChildTaskView>,
    /// 子任务指向的父任务 ID。
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// 网格子任务摘要（父任务查询视图）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildTaskView {
    pub task_id: String,
    pub state: TaskState,
    #[serde(default)]
    pub result_hash: Option<String>,
    /// 子配置的确定性哈希（网格内唯一）。
    #[serde(default)]
    pub config_hash: Option<String>,
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
    pub const RUN_COMPLETED: &str = "RunCompleted";
    pub const GRID_RESOLVED: &str = "GridResolved";
    pub const PLAN_READY: &str = "PlanReady";
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

/// QueryRows 表名（S12：members/excluded/unknown；S14 扩展：equity/holdings/fills）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RowsTable {
    Members,
    Excluded,
    Unknown,
    Equity,
    Holdings,
    Fills,
    Plan,
}

impl RowsTable {
    /// 预览桶对应的行判定值（运行产物桶返回 None）。
    pub fn verdict(&self) -> Option<&'static str> {
        match self {
            Self::Members => Some("pass"),
            Self::Excluded => Some("exclude"),
            Self::Unknown => Some("unknown"),
            Self::Equity | Self::Holdings | Self::Fills | Self::Plan => None,
        }
    }
}

/// QueryRows 行：按表名区分的类型化记录（serde untagged，JSON 形状与契约一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RowsRow {
    Member(crate::universe::MemberRow),
    Equity(crate::worker_api::EquityDoc),
    Holding(crate::worker_api::HoldingsDoc),
    Fill(crate::worker_api::FillDoc),
    Plan(crate::plan::PlanRow),
}

/// QueryRows 响应：cursor 绑定对象哈希，跨版本偏移拒绝。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowsPage {
    pub rows: Vec<RowsRow>,
    pub next_cursor: Option<String>,
    pub object_hash: String,
    pub total: u64,
}

// ---------------------------------------------------------------- S14 比较

/// 比较视图：完整（各自区间）或交集（另列起止日）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareView {
    Full,
    Intersection,
}

/// CompareRuns 请求（S14）：2..5 个不同且已完成的运行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareSpec {
    pub run_ids: Vec<String>,
    pub view: CompareView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark_snapshot_id: Option<String>,
}

/// 日期区间（交集视图的起止日）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: String,
    pub end: String,
}

/// 运行间差异（区间／费用／数据／模式），显式列出，不做静默排名（ST-S14-01）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Difference {
    /// region|cost|data|mode
    pub kind: String,
    pub detail: String,
}

/// 单个运行的指标视图（曲线经结果产物哈希引用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetrics {
    pub run_id: String,
    pub result_hash: String,
    pub start: String,
    pub end: String,
    pub mode: String,
    pub snapshot_id: String,
    pub universe_id: String,
    pub commission_rate: String,
    pub metrics: crate::metrics::MetricsSet,
}

/// 基准指标视图（基准缺日期时各指标为空并附原因，策略指标不受影响）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub snapshot_id: String,
    pub metrics: crate::metrics::MetricsSet,
}

/// CompareRuns 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub runs: Vec<RunMetrics>,
    pub differences: Vec<Difference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap: Option<DateRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkMetrics>,
}

// ---------------------------------------------------------------- S13 研究运行

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrategyTemplate {
    Ema,
    Momentum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rebalance {
    Daily,
    Weekly,
    Monthly,
}

/// 策略规格（契约 StrategySpec；ema/momentum 模板参数互斥可选）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySpec {
    pub template: StrategyTemplate,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slow: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookback: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip: Option<u32>,
    pub top_k: u32,
    pub rebalance: Rebalance,
    /// 首版仅支持 equal_slots（等槽位权重）。
    pub weight: String,
    /// 首版仅支持 point_in_time_adjusted。
    pub signal_basis: String,
}

/// 成本假设（契约 CostSpec）：费率用十进制字符串；confirmed 必须 true
/// （研究假设并非券商报价）；effective_schedule 覆盖整个实验区间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSpec {
    pub commission_rate: String,
    pub min_commission_cny: String,
    pub sell_tax_rate: String,
    pub other_fee_rate: String,
    pub slippage_bps: String,
    pub participation_rate: String,
    pub effective_schedule: Vec<EffectiveRange>,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveRange {
    pub start: String,
    pub end: String,
}

/// SubmitRun 请求（S13）。`grid` 为参数枚举，笛卡尔积 ≤100，子运行各自存档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSpec {
    pub snapshot_id: String,
    pub universe_id: String,
    pub strategy: StrategySpec,
    pub start: String,
    pub end: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_start: Option<String>,
    pub capital_cny: String,
    pub costs: CostSpec,
    pub rules_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark_snapshot_id: Option<String>,
    pub mode: UniverseMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<std::collections::BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    pub seed: u64,
}

impl RunSpec {
    /// 提交前校验（UT-S13-06）：任何失败都不入队、无子任务启动。
    pub fn validate(&self) -> crate::error::Result<()> {
        parse_trade_date(&self.start).map_err(|e| e.with_field("start"))?;
        parse_trade_date(&self.end).map_err(|e| e.with_field("end"))?;
        if self.start > self.end {
            return Err(ResearchError::invalid("开始日期不能晚于结束日期").with_field("start"));
        }
        // 样本外约束：test_start 必须晚于 training_end
        match (&self.training_end, &self.test_start) {
            (Some(t), Some(oos)) => {
                parse_trade_date(t).map_err(|e| e.with_field("training_end"))?;
                parse_trade_date(oos).map_err(|e| e.with_field("test_start"))?;
                if oos <= t {
                    return Err(ResearchError::invalid(format!(
                        "样本外开始日 {oos} 必须晚于训练结束日 {t}"
                    ))
                    .with_field("test_start"));
                }
            }
            (None, Some(_)) => {
                return Err(ResearchError::invalid("设置样本外区间必须同时给出 training_end")
                    .with_field("training_end"));
            }
            _ => {}
        }
        // 策略参数
        let params = match self.strategy.template {
            StrategyTemplate::Ema => crate::indicators::IndicatorParams::Ema {
                fast: self.strategy.fast.unwrap_or(0),
                slow: self.strategy.slow.unwrap_or(0),
            },
            StrategyTemplate::Momentum => crate::indicators::IndicatorParams::Momentum {
                lookback: self.strategy.lookback.unwrap_or(0),
                skip: self.strategy.skip.unwrap_or(0),
            },
        };
        crate::indicators::validate_indicator_params(
            serde_json::to_value(self.strategy.template)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
                .as_str(),
            &params,
        )?;
        if !(1..=500).contains(&self.strategy.top_k) {
            return Err(ResearchError::invalid("top_k 须在 1..=500").with_field("strategy.top_k"));
        }
        if self.strategy.weight != "equal_slots" {
            return Err(ResearchError::invalid("首版仅支持 equal_slots 权重").with_field("strategy.weight"));
        }
        if self.strategy.signal_basis != "point_in_time_adjusted" {
            return Err(ResearchError::invalid("首版仅支持 point_in_time_adjusted 信号口径")
                .with_field("strategy.signal_basis"));
        }
        // 资金与费率
        let capital = rust_decimal::Decimal::from_str_exact(&self.capital_cny).map_err(|_| {
            ResearchError::invalid("capital_cny 不是十进制数").with_field("capital_cny")
        })?;
        if capital.is_sign_negative() || capital.is_zero() {
            return Err(ResearchError::invalid("capital_cny 必须为正").with_field("capital_cny"));
        }
        self.validate_costs()?;
        // 规则包哈希
        if self.rules_hash.len() != 64 {
            return Err(ResearchError::invalid("rules_hash 必须是 SHA256 十六进制").with_field("rules_hash"));
        }
        // 网格：参数名白名单 + 笛卡尔积 ≤100
        if let Some(grid) = &self.grid {
            let mut combos: u64 = 1;
            for (name, values) in grid {
                if !["fast", "slow", "lookback", "skip", "top_k"].contains(&name.as_str()) {
                    return Err(ResearchError::invalid(format!("网格参数不支持：{name}"))
                        .with_field("grid"));
                }
                if values.is_empty() {
                    return Err(ResearchError::invalid(format!("网格参数 {name} 枚举为空")).with_field("grid"));
                }
                for v in values {
                    if v.parse::<u32>().is_err() {
                        return Err(ResearchError::invalid(format!("网格参数 {name} 的值不是非负整数：{v}"))
                            .with_field("grid"));
                    }
                }
                combos = combos.saturating_mul(values.len() as u64);
            }
            if combos > 100 {
                return Err(ResearchError::invalid(format!("网格组合数 {combos} 超过上限 100"))
                    .with_field("grid"));
            }
        }
        Ok(())
    }

    fn validate_costs(&self) -> crate::error::Result<()> {
        let rate = |s: &str, name: &str, max: &str| -> crate::error::Result<()> {
            let v = rust_decimal::Decimal::from_str_exact(s).map_err(|_| {
                ResearchError::invalid(format!("{name} 不是十进制数：{s}")).with_field(name)
            })?;
            let hi = rust_decimal::Decimal::from_str_exact(max).unwrap();
            if v.is_sign_negative() || v > hi {
                return Err(ResearchError::invalid(format!("{name} 须在 0..{max}")).with_field(name));
            }
            Ok(())
        };
        rate(&self.costs.commission_rate, "costs.commission_rate", "1")?;
        rate(&self.costs.min_commission_cny, "costs.min_commission_cny", "1000000000")?;
        rate(&self.costs.sell_tax_rate, "costs.sell_tax_rate", "1")?;
        rate(&self.costs.other_fee_rate, "costs.other_fee_rate", "1")?;
        rate(&self.costs.slippage_bps, "costs.slippage_bps", "1000")?;
        rate(&self.costs.participation_rate, "costs.participation_rate", "1")?;
        if !self.costs.confirmed {
            return Err(ResearchError::invalid(
                "成本假设必须显式确认（confirmed=true）；研究假设并非券商报价",
            )
            .with_field("costs.confirmed"));
        }
        // 生效区间覆盖整个实验（税费不允许静默缺省）
        if self.costs.effective_schedule.is_empty() {
            return Err(ResearchError::invalid("成本生效区间不能为空").with_field("costs.effective_schedule"));
        }
        let first = &self.costs.effective_schedule[0];
        let last = &self.costs.effective_schedule[self.costs.effective_schedule.len() - 1];
        parse_trade_date(&first.start).map_err(|e| e.with_field("costs.effective_schedule"))?;
        parse_trade_date(&last.end).map_err(|e| e.with_field("costs.effective_schedule"))?;
        if first.start > self.start || last.end < self.end {
            return Err(ResearchError::invalid(format!(
                "成本生效区间须覆盖实验 {}~{}",
                self.start, self.end
            ))
            .with_field("costs.effective_schedule"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- S15 交易计划

/// GeneratePlan 请求（S15）：复用策略与快照，产出下一交易日人工执行参考。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSpec {
    pub strategy: StrategySpec,
    pub snapshot_id: String,
    pub universe_id: String,
    pub as_of: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holding_version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holdings: Option<crate::plan::HoldingInput>,
    pub costs: CostSpec,
    pub rules_hash: String,
    /// 默认 false：as_of 早于最近已结束交易日时 STALE_DATA（UT-S15-04）。
    #[serde(default)]
    pub allow_historical: bool,
    pub mode: UniverseMode,
}
