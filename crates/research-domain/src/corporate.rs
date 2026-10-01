//! 公司行为应用、日历调仓日与规则包生效区间解析（S13 批次 3d）。
//!
//! 全部为纯函数：
//! - 拆股在除权日调整持仓（股数 × 比例），净值不凭空减半（UT-S13-08）；
//! - 现金分红在除息日计应收、支付日转现金，总资产不重复增加（UT-S13-11）；
//! - weekly/monthly 调仓只在周期最后一个交易日形成信号，下一交易日执行（UT-S13-10）；
//! - 规则包按生效区间解析，严格模式缺失即失败并定位板块/日期（UT-S13-12）。

use rust_decimal::Decimal;

use crate::{
    auxiliary::{ActionRecord, CalendarRecord, RuleRecord},
    error::{ErrorCode, ResearchError, Result},
    gate::BoardRules,
};

/// 公司行为在除权除息日的持仓/应收效果。
#[derive(Debug, Clone, PartialEq)]
pub struct ActionEffect {
    /// 拆股后持仓（held × split_ratio；无拆股则原值）。
    pub new_held: u64,
    /// 除息日新增应收股息（held × cash_per_share）；支付日转现金。
    pub dividend_receivable: Decimal,
}

/// 应用某一除权除息日的公司行为（仅当该行为已公告：announced_at ≤ 当日）。
/// 事件只按可见生效日应用——未公告的行为不参与计算（时点纪律）。
pub fn apply_action(
    held: u64,
    action: &ActionRecord,
    today: &str,
) -> Option<ActionEffect> {
    if action.ex_date != today || action.announced_at.as_str() > today {
        return None;
    }
    let new_held = match action.split_ratio {
        Some(ratio) => {
            let scaled = Decimal::from(held) * ratio;
            // 股数为整数：向下取整（零碎股按现金处理不在研究首版范围）
            scaled.floor().to_u64_saturating()
        }
        None => held,
    };
    let dividend_receivable = match action.cash_per_share {
        Some(per_share) => Decimal::from(held) * per_share,
        None => Decimal::ZERO,
    };
    Some(ActionEffect {
        new_held,
        dividend_receivable,
    })
}

trait ToU64Floor {
    fn to_u64_saturating(&self) -> u64;
}

impl ToU64Floor for Decimal {
    fn to_u64_saturating(&self) -> u64 {
        use rust_decimal::prelude::ToPrimitive;
        self.to_u64().unwrap_or(u64::MAX)
    }
}

/// 股息台账：除息日计应收、支付日转现金（转出后应收清零，不重复计入总资产）。
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DividendLedger {
    /// 应收未收股息（除息日～支付日之间计入总资产一次）。
    pub receivable: Decimal,
    /// 已转现金的累计股息。
    pub paid: Decimal,
}

/// 推进一日股息台账：除息日应收 +，支付日应收转现金。
/// 返回当日现金增量（仅支付日非零）。
pub fn advance_dividends(
    ledger: &mut DividendLedger,
    action: &ActionRecord,
    held: u64,
    today: &str,
) -> Decimal {
    // 除息日：登记应收（公告可见才计）
    if action.ex_date == today
        && action.announced_at.as_str() <= today
        && let Some(per_share) = action.cash_per_share
    {
        ledger.receivable += Decimal::from(held) * per_share;
    }
    // 支付日：应收转现金（转多少清多少，总资产不重复增加）
    if action.pay_date.as_deref() == Some(today) && ledger.receivable > Decimal::ZERO {
        let paid = ledger.receivable;
        ledger.receivable -= paid;
        ledger.paid += paid;
        return paid;
    }
    Decimal::ZERO
}

// ---------------------------------------------------------------- 日历调仓

/// 计算调仓信号日序列：daily=每个交易日；weekly/monthly=周期内最后一个交易日。
/// 日历不靠工作日猜测：节假日周末天然不在日历中，周期尾自动顺延到最后交易日。
/// 返回的每个信号日的执行日是日历中的下一个交易日（隔日执行）。
pub fn rebalance_signal_dates(
    calendar: &[CalendarRecord],
    rebalance: crate::protocol::Rebalance,
    start: &str,
    end: &str,
) -> Vec<String> {
    let mut dates: Vec<&str> = calendar
        .iter()
        .map(|c| c.trade_date.as_str())
        .filter(|d| d >= &start && d <= &end)
        .collect();
    dates.sort_unstable();
    match rebalance {
        crate::protocol::Rebalance::Daily => dates.into_iter().map(str::to_string).collect(),
        crate::protocol::Rebalance::Weekly | crate::protocol::Rebalance::Monthly => {
            let monthly = rebalance == crate::protocol::Rebalance::Monthly;
            let mut last_of_period: Option<(&str, (i32, u32))> = None;
            let mut out = Vec::new();
            for d in &dates {
                let key = period_key(d, monthly);
                match last_of_period {
                    Some((_, pk)) if pk != key => {
                        if let Some((last, _)) = last_of_period {
                            out.push(last.to_string());
                        }
                        last_of_period = Some((d, key));
                    }
                    _ => last_of_period = Some((d, key)),
                }
            }
            if let Some((last, _)) = last_of_period {
                out.push(last.to_string());
            }
            out
        }
    }
}

/// 周期键：weekly=ISO 周（年, 周）；monthly=（年, 月）。日期格式 YYYY-MM-DD 的字典序
/// 支持直接截取：monthly 用 [0..7]，weekly 用（年, 自年初第几周）。
fn period_key(date: &str, monthly: bool) -> (i32, u32) {
    let year: i32 = date[..4].parse().unwrap_or(0);
    if monthly {
        let month: u32 = date[5..7].parse().unwrap_or(0);
        (year, month * 100) // 与 weekly 的周序空间隔离
    } else {
        (year, week_of_year(date))
    }
}

/// 周序（周一为一周开始）。儒略日数 0 是周一，直接除 7 即得周一起始的分桶。
fn week_of_year(date: &str) -> u32 {
    let y: i32 = date[..4].parse().unwrap_or(1970);
    let m: u32 = date[5..7].parse().unwrap_or(1);
    let d: u32 = date[8..10].parse().unwrap_or(1);
    let days = julian_day(y, m, d);
    (days / 7) as u32
}

fn julian_day(y: i32, m: u32, d: u32) -> i64 {
    let a = (14 - m as i64) / 12;
    let y2 = y as i64 + 4800 - a;
    let m2 = m as i64 + 12 * a - 3;
    d as i64 + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
}

/// 信号日的执行日：日历中严格晚于 signal_date 的下一个交易日；无则返回 None（不执行）。
pub fn next_trade_date(calendar: &[CalendarRecord], signal_date: &str) -> Option<String> {
    calendar
        .iter()
        .map(|c| c.trade_date.as_str())
        .filter(|d| d > &signal_date)
        .min()
        .map(str::to_string)
}

// ---------------------------------------------------------------- 规则包

/// 按板块与交易日解析生效规则：覆盖 start ≤ date < end（开放区间无上界）。
/// 严格模式缺失即失败，错误定位板块与日期（UT-S13-12：不以默认规则静默代替）。
pub fn resolve_rules(
    rules: &[RuleRecord],
    board: &str,
    date: &str,
) -> Result<BoardRules> {
    let hit = rules
        .iter()
        .find(|r| {
            r.board == board
                && r.effective_start.as_str() <= date
                && r.effective_end.as_deref().is_none_or(|e| date < e)
        })
        .ok_or_else(|| {
            ResearchError::new(
                ErrorCode::MissingCapability,
                format!("严格任务失败：规则包缺少板块 {board} 在 {date} 的生效区间（不得以现行规则反套历史或默认值代替）"),
            )
        })?;
    Ok(BoardRules {
        board: hit.board.clone(),
        tick: hit.tick,
        min_qty: hit.min_qty,
        qty_step: hit.qty_step,
        limit_pct: hit.limit_pct,
    })
}

/// 区间覆盖完整性检查：运行区间内每一天都必须有生效规则。
/// 返回第一个缺失日（配合 resolve_rules 的错误定位标的/日期）。
pub fn first_missing_rule_date(
    rules: &[RuleRecord],
    board: &str,
    calendar: &[CalendarRecord],
    start: &str,
    end: &str,
) -> Option<String> {
    calendar
        .iter()
        .map(|c| c.trade_date.as_str())
        .filter(|d| d >= &start && d <= &end)
        .find(|d| resolve_rules(rules, board, d).is_err())
        .map(str::to_string)
}
