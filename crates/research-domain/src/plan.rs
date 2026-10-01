//! 交易计划纯函数（S15）：目标仓位数学、持仓校验、CSV 转义与计划文档。
//!
//! 计划只产生人工执行参考，绝不调用下单通道（FR-R09）。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, ResearchError, Result};

/// 手工持仓输入的单一标的（契约 HoldingInput.positions 项）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionInput {
    pub instrument_id: String,
    pub quantity: u64,
    pub sellable_quantity: u64,
}

/// 手工持仓输入（契约 HoldingInput）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldingInput {
    pub as_of: String,
    /// 非负（UT-S15-02：为负拒绝并定位字段）。
    pub cash_cny: String,
    pub positions: Vec<PositionInput>,
    /// 正数；与现金及参考估值对账。
    pub total_assets_cny: String,
}

impl HoldingInput {
    /// 校验：现金非负、0 ≤ 可卖 ≤ 持有、总资产为正；错误定位字段（UT-S15-02）。
    pub fn validate(&self) -> Result<()> {
        let cash = Decimal::from_str_exact(&self.cash_cny)
            .map_err(|_| ResearchError::invalid("cash_cny 非十进制").with_field("cash_cny"))?;
        if cash < Decimal::ZERO {
            return Err(ResearchError::invalid("现金为负，拒绝生成计划")
                .with_field("cash_cny"));
        }
        for (i, p) in self.positions.iter().enumerate() {
            if p.sellable_quantity > p.quantity {
                return Err(ResearchError::invalid(format!(
                    "可卖数量 {} 超过持有 {}（{}）",
                    p.sellable_quantity, p.quantity, p.instrument_id
                ))
                .with_field(&format!("positions[{i}].sellable_quantity")));
            }
        }
        let total = Decimal::from_str_exact(&self.total_assets_cny)
            .map_err(|_| ResearchError::invalid("total_assets_cny 非十进制")
                .with_field("total_assets_cny"))?;
        if total <= Decimal::ZERO {
            return Err(ResearchError::invalid("总资产必须为正").with_field("total_assets_cny"));
        }
        Ok(())
    }
}

/// 目标仓位结果（UT-S15-01 的数学核心）。
#[derive(Debug, Clone, PartialEq)]
pub struct PlanTarget {
    /// 目标股数（按步长向下取整）。
    pub target_quantity: u64,
    /// 建议差额（正买负卖）。
    pub delta_quantity: i64,
    /// 差额参考金额（|delta| × 参考价）。
    pub reference_value_cny: Decimal,
}

/// 目标仓位：目标 = floor(总资产×权重/参考价/步长)×步长；差额 = 目标 - 持有。
/// UT-S15-01：10000×0.2/10=200 股，持 100 → 建议买 100 股、参考额 1000。
pub fn plan_target(
    total_assets: Decimal,
    weight: Decimal,
    price: Decimal,
    held: u64,
    qty_step: u64,
) -> Result<PlanTarget> {
    if price <= Decimal::ZERO {
        return Err(ResearchError::invalid("参考价必须为正").with_field("reference_price"));
    }
    if qty_step == 0 {
        return Err(ResearchError::invalid("步长必须为正").with_field("qty_step"));
    }
    if weight < Decimal::ZERO {
        return Err(ResearchError::invalid("目标权重不能为负").with_field("target_weight"));
    }
    let budget = total_assets * weight;
    let raw = budget / price;
    let step = Decimal::from(qty_step);
    let lots = (raw / step).floor();
    let target = lots * step;
    let target_quantity = target.to_u64().unwrap_or(u64::MAX);
    let delta = target_quantity as i64 - held as i64;
    let reference = Decimal::from(delta.unsigned_abs()) * price;
    Ok(PlanTarget { target_quantity, delta_quantity: delta, reference_value_cny: reference })
}

/// CSV 自由文本转义：公式前缀（= + - @）加单引号防注入；含分隔符/引号/换行加引号包裹。
pub fn csv_escape(field: &str) -> String {
    let dangerous = field.starts_with(['=', '+', '-', '@']);
    let special = field.contains([',', '"', '\n', '\r']);
    let body = if dangerous { format!("'{field}") } else { field.to_string() };
    if dangerous || special {
        format!("\"{}\"", body.replace('"', "\"\""))
    } else {
        body
    }
}

/// 计划行（契约 plan 行类型）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRow {
    pub instrument_id: String,
    /// 首版无主档名称时与代码一致。
    pub name: String,
    pub target_weight: String,
    pub current_quantity: u64,
    pub sellable_quantity: u64,
    pub target_quantity: u64,
    /// 正买负卖。
    pub delta_quantity: i64,
    pub reference_price: String,
    pub reason: String,
    #[serde(default)]
    pub limitations: Vec<String>,
}

/// 交易计划文档（内容寻址 JSON；kind=trade_plan）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradePlanDoc {
    pub kind: String,
    pub snapshot_id: String,
    pub universe_id: String,
    pub as_of: String,
    /// 历史计划标识（UT-S15-04：as_of 早于最近已结束交易日且已显式确认）。
    pub historical: bool,
    pub strategy_version: String,
    pub mode: String,
    pub cash_cny: String,
    pub total_assets_cny: String,
    pub rows: Vec<PlanRow>,
    /// 数据版本（快照清单哈希）与限制说明，导出必含（ST-S15-01）。
    pub data_version: String,
    pub limitations: Vec<String>,
}

impl TradePlanDoc {
    /// 计划 CSV 字节（UTF-8 BOM + 表头 + 行；历史计划与数据版本/限制说明逐行标注）。
    pub fn csv_bytes(&self) -> Vec<u8> {
        let mut out = String::from("\u{FEFF}");
        out.push_str("instrument_id,name,target_weight,current_quantity,sellable_quantity,target_quantity,delta_quantity,reference_price,reason,limitations,plan_limitations,data_version,historical\n");
        for r in &self.rows {
            let line = [
                r.instrument_id.as_str(),
                r.name.as_str(),
                r.target_weight.as_str(),
                &r.current_quantity.to_string(),
                &r.sellable_quantity.to_string(),
                &r.target_quantity.to_string(),
                &r.delta_quantity.to_string(),
                r.reference_price.as_str(),
                r.reason.as_str(),
                &r.limitations.join("; "),
                &self.limitations.join("; "),
                self.data_version.as_str(),
                if self.historical { "historical-plan" } else { "" },
            ]
            .map(csv_escape)
            .join(",");
        out.push_str(&line);
        out.push('\n');
        }
        out.into_bytes()
    }
}

use rust_decimal::prelude::ToPrimitive;

/// 手工备注（契约 ManualNote）：追加独立记录，不更改模拟成交。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualNote {
    pub plan_id: String,
    /// 1..=2000 字符。
    pub text: String,
    /// UTC 时间（RFC3339）。
    pub timestamp: String,
    /// 备注 | 已人工处理 | 放弃（不作自动成交）。
    pub kind: String,
}

impl ManualNote {
    pub fn validate(&self) -> Result<()> {
        let len = self.text.chars().count();
        if !(1..=2000).contains(&len) {
            return Err(ResearchError::invalid(format!("备注长度 {len} 超出 1..=2000"))
                .with_field("text"));
        }
        if !matches!(self.kind.as_str(), "备注" | "已人工处理" | "放弃") {
            return Err(ResearchError::invalid(format!("未知备注类型：{}", self.kind))
                .with_field("kind"));
        }
        Ok(())
    }
}

/// 备注回执（契约 NoteRef）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRef {
    pub note_id: String,
    pub plan_id: String,
}

/// 导出请求（契约 ExportSpec）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSpec {
    pub plan_id: String,
    pub destination: String,
    #[serde(default)]
    pub overwrite_confirmed: bool,
    /// 首版仅 csv_utf8_bom。
    pub format: String,
}

/// 导出回执（契约 ExportReceipt）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportReceipt {
    pub export_id: String,
    pub path: String,
    pub sha256: String,
    pub rows: u64,
}

/// 计划规格校验（提交时；UT-S15-02 定位字段）。
pub fn validate_plan_holdings(h: &HoldingInput) -> Result<()> {
    h.validate()
}

/// 计划生成前置：as_of 早于快照最近已结束交易日 → 默认 STALE_DATA（UT-S15-04）。
pub fn check_stale_as_of(as_of: &str, latest_trade_date: &str, allow_historical: bool) -> Result<bool> {
    if as_of < latest_trade_date {
        if !allow_historical {
            return Err(ResearchError::new(
                ErrorCode::StaleData,
                format!("计划日 {as_of} 早于最近已结束交易日 {latest_trade_date}；生成历史计划需显式确认"),
            )
            .with_field("as_of"));
        }
        return Ok(true);
    }
    Ok(false)
}
