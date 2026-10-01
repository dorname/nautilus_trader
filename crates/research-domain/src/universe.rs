//! 股票池规则（RuleAST）与三态时点评估（S12）。
//!
//! 三态语义（core-S12-test-cases.md）：每个条件为 命中/不满足/未知；
//! OR 组任一命中即命中，否则任一未知即未知；AND 根任一不满足即不满足，
//! 否则任一未知即未知。数据缺失产出未知，绝不用 0 或替代口径伪造。
//! 时点纪律：财务只取 available_at ≤ as_of 的报告（同报告期取最大修订号）；
//! 价格取 as_of 当日或之前最近交易日；主档按 as_of 当时状态判断在册。

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    auxiliary::{FinancialRecord, MasterRecord},
    error::{ErrorCode, ResearchError, Result},
    quotes::QuoteRow,
    time::trade_date_to_date32,
};

/// 规则字段（契约 RuleAST.field）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleField {
    Board,
    ListedDays,
    Close,
    AvgAmount,
    Roe,
    Eps,
    Bps,
    NetProfit,
    RevenueGrowth,
    DebtRatio,
}

impl RuleField {
    /// 该字段依赖的数据能力（严格预检用）。
    pub fn capability(&self) -> &'static str {
        match self {
            Self::Board | Self::ListedDays => "aux.master",
            Self::Close | Self::AvgAmount => "quotes.daily",
            _ => "aux.financial",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Board => "板块",
            Self::ListedDays => "上市天数",
            Self::Close => "收盘价",
            Self::AvgAmount => "平均成交额",
            Self::Roe => "ROE",
            Self::Eps => "EPS",
            Self::Bps => "BPS",
            Self::NetProfit => "净利润",
            Self::RevenueGrowth => "营收增速",
            Self::DebtRatio => "资产负债率",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleOp {
    Gte,
    Lte,
    Between,
    In,
}

/// 条件：field + op + 数值/集合 + 可选窗口（avg_amount 必需）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub field: RuleField,
    pub op: RuleOp,
    /// gte/lte 的数值字符串（十进制）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// between 的 [下界, 上界] 或 in 的枚举集合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    /// avg_amount 的窗口（交易日，1..=2500）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<u32>,
}

/// OR 组：只允许包条件，不允许再嵌套组（最大深度 2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrGroup {
    pub op: OrOp,
    pub children: Vec<Condition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "or")]
pub enum OrOp {
    Or,
}

/// 根 AND 的子节点：条件或 OR 组。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleChild {
    Cond(Condition),
    Or(OrGroup),
}

/// RuleAST 根：固定 AND。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleGroup {
    pub op: AndOp,
    pub children: Vec<RuleChild>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "and")]
pub enum AndOp {
    And,
}

pub const MAX_RULE_CHILDREN: usize = 20;

/// 规则静态校验（契约 RuleAST.constraints）。
pub fn validate_rule(rule: &RuleGroup) -> Result<()> {
    if rule.children.is_empty() {
        return Err(ResearchError::invalid("规则至少需要一个条件").with_field("rule.children"));
    }
    if rule.children.len() > MAX_RULE_CHILDREN {
        return Err(ResearchError::invalid(format!(
            "规则子节点 {} 超过上限 {MAX_RULE_CHILDREN}",
            rule.children.len()
        ))
        .with_field("rule.children"));
    }
    for child in &rule.children {
        match child {
            RuleChild::Cond(c) => validate_condition(c)?,
            RuleChild::Or(g) => {
                if g.children.is_empty() {
                    return Err(ResearchError::invalid("OR 组至少需要一个条件").with_field("rule.children"));
                }
                if g.children.len() > MAX_RULE_CHILDREN {
                    return Err(ResearchError::invalid("OR 组条件过多").with_field("rule.children"));
                }
                for c in &g.children {
                    validate_condition(c)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_condition(c: &Condition) -> Result<()> {
    let field_path = format!("rule.{}", serde_json::to_value(c.field).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default());
    match c.op {
        RuleOp::Gte | RuleOp::Lte => {
            let v = c.value.as_deref().ok_or_else(|| {
                ResearchError::invalid("gte/lte 需要 value 数值").with_field(field_path.clone())
            })?;
            Decimal::from_str_exact(v).map_err(|_| {
                ResearchError::invalid(format!("条件数值不是十进制：{v}")).with_field(field_path.clone())
            })?;
            if c.values.is_some() {
                return Err(ResearchError::invalid("gte/lte 不接受 values 集合").with_field(field_path));
            }
        }
        RuleOp::Between => {
            let vs = c.values.as_deref().ok_or_else(|| {
                ResearchError::invalid("between 需要 values [下界, 上界]").with_field(field_path.clone())
            })?;
            if vs.len() != 2 {
                return Err(ResearchError::invalid("between 需要恰好两个界值").with_field(field_path.clone()));
            }
            let lo = Decimal::from_str_exact(&vs[0]).map_err(|_| {
                ResearchError::invalid(format!("between 下界不是十进制：{}", vs[0])).with_field(field_path.clone())
            })?;
            let hi = Decimal::from_str_exact(&vs[1]).map_err(|_| {
                ResearchError::invalid(format!("between 上界不是十进制：{}", vs[1])).with_field(field_path.clone())
            })?;
            if lo > hi {
                return Err(ResearchError::invalid("between 下界不能大于上界").with_field(field_path));
            }
        }
        RuleOp::In => {
            let vs = c.values.as_deref().ok_or_else(|| {
                ResearchError::invalid("in 需要 values 枚举集合").with_field(field_path.clone())
            })?;
            if vs.is_empty() {
                return Err(ResearchError::invalid("in 集合不能为空").with_field(field_path));
            }
        }
    }
    if c.field == RuleField::Board && c.op != RuleOp::In {
        return Err(ResearchError::invalid("board 只能使用 in 操作").with_field("rule.board"));
    }
    if c.field == RuleField::AvgAmount {
        match c.window {
            Some(w) if (1..=2500).contains(&w) => {}
            _ => {
                return Err(ResearchError::invalid("avg_amount 需要 1..=2500 的窗口")
                    .with_field("rule.avg_amount.window"))
            }
        }
    } else if c.window.is_some() {
        return Err(ResearchError::invalid("仅 avg_amount 允许窗口").with_field(field_path));
    }
    Ok(())
}

/// 三态判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tri {
    Hit,
    Miss,
    Unknown,
}

/// 单条件评估输出：判定 + 原因（不满足/未知时给出）+ 实际取值（证据）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CondOutcome {
    pub verdict: Tri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// 池成员行（预览产物与 QueryRows members/excluded/unknown 的行型）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberRow {
    pub instrument_id: String,
    pub name: String,
    pub as_of: String,
    pub verdict: String,
    pub reasons: Vec<String>,
    pub field_values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_at: Option<String>,
}

/// 评估上下文：某快照在 as_of 时点可见的全部数据。
pub struct EvalContext<'a> {
    pub as_of: &'a str,
    pub master: Option<&'a BTreeMap<String, MasterRecord>>,
    pub financial: Option<&'a BTreeMap<String, Vec<FinancialRecord>>>,
    /// 每个标的按交易日升序的行情。
    pub quotes: Option<&'a BTreeMap<String, Vec<QuoteRow>>>,
    /// ignore_condition（仅探索模式）：未知条件从组合中剔除。
    pub ignore_unknown_conditions: bool,
}

/// 数值比较：gte/lte/between。in 仅用于 board（字符串集合）。
fn compare_num(c: &Condition, actual: Decimal) -> Tri {
    match c.op {
        RuleOp::Gte => {
            let want = Decimal::from_str_exact(c.value.as_deref().unwrap_or_default());
            match want {
                Ok(w) => if actual >= w { Tri::Hit } else { Tri::Miss },
                Err(_) => Tri::Unknown,
            }
        }
        RuleOp::Lte => {
            let want = Decimal::from_str_exact(c.value.as_deref().unwrap_or_default());
            match want {
                Ok(w) => if actual <= w { Tri::Hit } else { Tri::Miss },
                Err(_) => Tri::Unknown,
            }
        }
        RuleOp::Between => {
            let vs = c.values.as_deref().unwrap_or(&[]);
            if vs.len() != 2 {
                return Tri::Unknown;
            }
            match (Decimal::from_str_exact(&vs[0]), Decimal::from_str_exact(&vs[1])) {
                (Ok(lo), Ok(hi)) => if actual >= lo && actual <= hi { Tri::Hit } else { Tri::Miss },
                _ => Tri::Unknown,
            }
        }
        RuleOp::In => Tri::Unknown,
    }
}

fn dec_label(v: Decimal) -> String {
    v.normalize().to_string()
}

/// 评估单个条件（纯函数，UT-S12-01/03/05 直接覆盖）。
pub fn eval_condition(c: &Condition, instrument: &str, ctx: &EvalContext) -> CondOutcome {
    let miss = |actual: String| CondOutcome {
        verdict: Tri::Miss,
        reason: Some(format!("{}不满足条件（实际 {actual}）", c.field.label())),
        value: Some(actual),
    };
    let hit = |actual: String| CondOutcome { verdict: Tri::Hit, reason: None, value: Some(actual) };
    let unknown = |reason: &str| CondOutcome {
        verdict: Tri::Unknown,
        reason: Some(format!("{}：{reason}", c.field.label())),
        value: None,
    };
    let judge = |actual: Decimal| match compare_num(c, actual) {
        Tri::Hit => hit(dec_label(actual)),
        Tri::Miss => miss(dec_label(actual)),
        Tri::Unknown => unknown("条件参数非法"),
    };

    match c.field {
        RuleField::Board => {
            let Some(master) = ctx.master else {
                return unknown("快照未导入证券主档");
            };
            let Some(rec) = master.get(instrument) else {
                return unknown("主档缺少该标的");
            };
            let set: Vec<&str> = c.values.as_deref().unwrap_or(&[]).iter().map(String::as_str).collect();
            if set.contains(&rec.board.as_str()) {
                hit(rec.board.clone())
            } else {
                miss(rec.board.clone())
            }
        }
        RuleField::ListedDays => {
            let Some(master) = ctx.master else {
                return unknown("快照未导入证券主档");
            };
            let Some(rec) = master.get(instrument) else {
                return unknown("主档缺少该标的");
            };
            let days = i64::from(trade_date_to_date32(ctx.as_of).unwrap_or(0))
                - i64::from(trade_date_to_date32(&rec.listed_date).unwrap_or(0));
            judge(Decimal::from(days.max(0)))
        }
        RuleField::Close => {
            let Some(quotes) = ctx.quotes else {
                return unknown("快照未导入日线行情");
            };
            let Some(rows) = quotes.get(instrument) else {
                return unknown("该标的在快照中无行情");
            };
            match rows.iter().rev().find(|r| r.trade_date.as_str() <= ctx.as_of) {
                Some(r) => judge(r.close),
                None => unknown("as_of 之前无行情"),
            }
        }
        RuleField::AvgAmount => {
            let Some(quotes) = ctx.quotes else {
                return unknown("快照未导入日线行情");
            };
            let window = c.window.unwrap_or(0) as usize;
            let Some(rows) = quotes.get(instrument) else {
                return unknown("该标的在快照中无行情");
            };
            let upto: Vec<&QuoteRow> = rows
                .iter()
                .rev()
                .filter(|r| r.trade_date.as_str() <= ctx.as_of)
                .take(window)
                .collect();
            if upto.len() < window {
                return unknown(&format!("窗口 {window} 内仅有 {} 个交易日行情", upto.len()));
            }
            // 成交额缺失即未知：绝不用 close×volume 替代（UT-S12-05）
            let mut sum = Decimal::ZERO;
            for r in &upto {
                match r.amount_cny {
                    Some(a) => sum += a,
                    None => return unknown(&format!("{} 成交额缺失", r.trade_date)),
                }
            }
            judge(sum / Decimal::from(window as u64))
        }
        // 财务字段：只取 available_at ≤ as_of 的报告；同报告期取最大修订号
        RuleField::Roe | RuleField::Eps | RuleField::Bps | RuleField::NetProfit
        | RuleField::RevenueGrowth | RuleField::DebtRatio => {
            let Some(fin) = ctx.financial else {
                return unknown("快照未导入财务数据");
            };
            let Some(recs) = fin.get(instrument) else {
                return unknown("该标的无财务报告");
            };
            let usable: Vec<&FinancialRecord> = recs
                .iter()
                .filter(|r| r.available_at.as_str() <= ctx.as_of)
                .collect();
            let Some(latest) = usable
                .iter()
                .max_by(|a, b| (&a.period_end, a.revision).cmp(&(&b.period_end, b.revision)))
            else {
                return unknown("as_of 时点尚无已公告报告");
            };
            let value = match c.field {
                RuleField::Roe => latest.roe,
                RuleField::Eps => latest.eps,
                RuleField::Bps => latest.bps,
                RuleField::NetProfit => latest.net_profit,
                RuleField::RevenueGrowth => latest.revenue_growth,
                RuleField::DebtRatio => latest.debt_ratio,
                _ => unreachable!(),
            };
            match value {
                Some(v) => judge(v),
                None => unknown(&format!("报告期 {} 未披露该指标", latest.period_end)),
            }
        }
    }
}

/// OR 组三态合成：任一命中→命中；否则任一未知（或全被忽略）→未知；否则不满足。
fn or_verdict(verdicts: &[Tri], ignore_unknown: bool) -> Tri {
    let eff: Vec<Tri> = verdicts
        .iter()
        .copied()
        .filter(|v| !(ignore_unknown && *v == Tri::Unknown))
        .collect();
    if eff.iter().any(|v| *v == Tri::Hit) {
        Tri::Hit
    } else if eff.is_empty() || eff.iter().any(|v| *v == Tri::Unknown) {
        Tri::Unknown
    } else {
        Tri::Miss
    }
}

/// AND 根三态合成：任一不满足→不满足；否则任一未知（或全被忽略）→未知；否则命中。
fn and_verdict(verdicts: &[Tri], ignore_unknown: bool) -> Tri {
    let eff: Vec<Tri> = verdicts
        .iter()
        .copied()
        .filter(|v| !(ignore_unknown && *v == Tri::Unknown))
        .collect();
    if eff.iter().any(|v| *v == Tri::Miss) {
        Tri::Miss
    } else if eff.is_empty() || eff.iter().any(|v| *v == Tri::Unknown) {
        Tri::Unknown
    } else {
        Tri::Hit
    }
}

fn field_key(field: RuleField) -> String {
    serde_json::to_value(field)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// 评估整棵规则树：返回（总判定，原因列表，字段取值证据）。
pub fn eval_group(
    rule: &RuleGroup,
    instrument: &str,
    ctx: &EvalContext,
) -> (Tri, Vec<String>, BTreeMap<String, String>) {
    let mut verdicts = Vec::new();
    let mut reasons = Vec::new();
    let mut field_values = BTreeMap::new();
    let collect = |c: &Condition,
                       o: &CondOutcome,
                       acc: &mut Vec<Tri>,
                       reasons: &mut Vec<String>,
                       values: &mut BTreeMap<String, String>| {
        if let Some(v) = &o.value {
            values.insert(field_key(c.field), v.clone());
        }
        if o.verdict != Tri::Hit {
            if let Some(r) = &o.reason {
                reasons.push(r.clone());
            }
        }
        acc.push(o.verdict);
    };
    for child in &rule.children {
        match child {
            RuleChild::Cond(c) => {
                let o = eval_condition(c, instrument, ctx);
                collect(c, &o, &mut verdicts, &mut reasons, &mut field_values);
            }
            RuleChild::Or(g) => {
                let mut sub = Vec::new();
                for c in &g.children {
                    let o = eval_condition(c, instrument, ctx);
                    collect(c, &o, &mut sub, &mut reasons, &mut field_values);
                }
                verdicts.push(or_verdict(&sub, ctx.ignore_unknown_conditions));
            }
        }
    }
    (and_verdict(&verdicts, ctx.ignore_unknown_conditions), reasons, field_values)
}

/// 候选标的：as_of 时点按主档在册（上市≤as_of 且未退市），无主档时退化为
/// 行情/财务中出现过的标的（探索模式兜底；严格模式在预检已拒绝缺主档）。
pub fn candidates(ctx: &EvalContext) -> Vec<String> {
    let mut set = std::collections::BTreeSet::new();
    match ctx.master {
        Some(master) => {
            for (id, rec) in master {
                let listed = rec.listed_date.as_str() <= ctx.as_of;
                let not_delisted = rec.delisted_date.as_deref().is_none_or(|d| d > ctx.as_of);
                if listed && not_delisted {
                    set.insert(id.clone());
                }
            }
        }
        None => {
            if let Some(q) = ctx.quotes {
                set.extend(q.keys().cloned());
            }
            if let Some(f) = ctx.financial {
                set.extend(f.keys().cloned());
            }
        }
    }
    set.into_iter().collect()
}

/// 运行预检（UT-S12-04）：固定池形成日（universe.as_of）晚于回测开始日 →
/// 严格拒绝并指出形成日；不静默回填成员。
pub fn precheck_run_universe(
    universe_as_of: &str,
    membership: &str,
    run_start: &str,
    mode: &str,
) -> Result<()> {
    if membership == "fixed" && mode == "strict" && universe_as_of > run_start {
        return Err(ResearchError::new(
            ErrorCode::InvalidArgument,
            format!("固定池形成日 {universe_as_of} 晚于回测开始日 {run_start}，严格模式拒绝；请改用更早形成的池或探索模式"),
        )
        .with_field("start"));
    }
    Ok(())
}
