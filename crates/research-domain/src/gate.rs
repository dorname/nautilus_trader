//! A股交易规则门与下单缩量（S13-AC-02，UT-S13-04/09/13）。
//!
//! 位于领域核心：规则包解析（corporate.rs::resolve_rules）产出 BoardRules，
//! 引擎工作器经 re-export 使用。
//!
//! 规则来自快照的规则表（tick/最小量/步长/涨跌停），按板块生效，
//! 不套用全市场统一 100 股假设。所有拒绝都带理由并进入运行产物。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 板块交易规则（测试合成规则不代表真实市场）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardRules {
    pub board: String,
    /// 最小报价单位（如 0.01）。
    pub tick: Decimal,
    /// 买入最小数量。
    pub min_qty: u64,
    /// 数量步长。
    pub qty_step: u64,
    /// 涨跌停幅度（None = 无涨跌停，如合成规则）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_pct: Option<Decimal>,
}

/// 拒绝理由（记录进运行产物，不静默丢弃）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Reject {
    /// 停牌：状态表当日停牌。
    Suspended { date: String },
    /// 缺价格：当日无可用开盘价。
    PriceMissing { date: String },
    /// 开盘即涨停，买入方向不可成交。
    LimitUp { open: String },
    /// 开盘即跌停，卖出方向不可成交。
    LimitDown { open: String },
    /// 低于板块最小数量。
    BelowMinQty { qty: u64, min: u64 },
    /// 不满足数量步长。
    BadStep { qty: u64, step: u64 },
    /// T+1：当日新买不可卖。
    NotSellableT1 { requested: u64, sellable: u64 },
    /// 含费成本超过可用现金（缩量到 0 时）。
    InsufficientCash { need: String, have: String },
}

impl Reject {
    /// 中文描述（日志与产物展示）。
    pub fn describe(&self) -> String {
        match self {
            Self::Suspended { date } => format!("{date} 停牌，拒绝委托"),
            Self::PriceMissing { date } => format!("{date} 无可用价格，拒绝委托"),
            Self::LimitUp { open } => format!("开盘 {open} 涨停，买入不可成交"),
            Self::LimitDown { open } => format!("开盘 {open} 跌停，卖出不可成交"),
            Self::BelowMinQty { qty, min } => format!("数量 {qty} 低于最小单位 {min}"),
            Self::BadStep { qty, step } => format!("数量 {qty} 不满足步长 {step}"),
            Self::NotSellableT1 { requested, sellable } => {
                format!("T+1：请求卖出 {requested}，可卖库存 {sellable}（当日新买不可卖）")
            }
            Self::InsufficientCash { need, have } => format!("含费成本 {need} 超过可用现金 {have}"),
        }
    }
}

/// 买入数量合法性：最小量 + 步长（UT-S13-09：板块规则 200/1 → 199 拒、201 过）。
pub fn validate_buy_qty(qty: u64, rules: &BoardRules) -> Result<(), Reject> {
    if qty < rules.min_qty {
        return Err(Reject::BelowMinQty { qty, min: rules.min_qty });
    }
    if rules.qty_step > 1 && qty % rules.qty_step != 0 {
        return Err(Reject::BadStep { qty, step: rules.qty_step });
    }
    Ok(())
}

/// 卖出数量合法性：T+1 可卖库存约束 + 步长（UT-S13-04：当日新买 100 立刻卖 → 拒）。
pub fn validate_sell_qty(qty: u64, sellable: u64, rules: &BoardRules) -> Result<(), Reject> {
    if qty > sellable {
        return Err(Reject::NotSellableT1 { requested: qty, sellable });
    }
    if qty == 0 {
        return Err(Reject::NotSellableT1 { requested: 0, sellable });
    }
    if rules.qty_step > 1 && qty % rules.qty_step != 0 {
        return Err(Reject::BadStep { qty, step: rules.qty_step });
    }
    Ok(())
}

/// 开盘可成交性预检（UT-S13-04）：停牌 / 缺价格 / 开盘达到方向涨跌停。
/// 返回可用开盘价；prev_close 为昨收（涨跌停判定基准）。
pub fn gate_open_price(
    is_sell: bool,
    date: &str,
    suspended: bool,
    open: Option<Decimal>,
    prev_close: Option<Decimal>,
    rules: &BoardRules,
) -> Result<Decimal, Reject> {
    if suspended {
        return Err(Reject::Suspended { date: date.to_string() });
    }
    let open = open.ok_or_else(|| Reject::PriceMissing { date: date.to_string() })?;
    if let (Some(limit), Some(prev)) = (rules.limit_pct, prev_close) {
        let up = prev * (Decimal::ONE + limit);
        let down = prev * (Decimal::ONE - limit);
        if !is_sell && open >= up {
            return Err(Reject::LimitUp { open: open.to_string() });
        }
        if is_sell && open <= down {
            return Err(Reject::LimitDown { open: open.to_string() });
        }
    }
    Ok(open)
}

/// 含费买入缩量（UT-S13-13）：目标数量按现金与含费成本逐档缩减到合法单位，
/// 现金绝不为负；缩到 0 即当日不买（未成交当日失效由调用方保证）。
/// `fee_of` 给定数量的预估含费成本（成交额+费用）。
pub fn size_buy<F>(cash: Decimal, est_price: Decimal, target_qty: u64, rules: &BoardRules, fee_of: F) -> u64
where
    F: Fn(u64) -> Decimal,
{
    if est_price.is_sign_negative() || est_price.is_zero() || rules.qty_step == 0 {
        return 0;
    }
    // 现金硬上限 → 步长对齐
    let affordable = cash / est_price;
    let max_by_cash = affordable.floor().to_string().parse::<u64>().unwrap_or(0);
    let mut qty = target_qty.min(max_by_cash - max_by_cash % rules.qty_step);
    while qty >= rules.min_qty {
        let cost = fee_of(qty);
        if cost <= cash {
            return qty;
        }
        qty = qty.saturating_sub(rules.qty_step);
    }
    0
}
