//! 组合指标纯函数（S14 比较）。
//!
//! 全部按完整净值序列计算，不做降采样（UT-S14-01）；不可计算的指标返回
//! None 并附原因，绝不产生 NaN/Infinity（UT-S14-02，十进制定点天然无 NaN）。

use rust_decimal::Decimal;

/// 可空指标：值为规范化十进制字符串，不可计算时 None + 原因。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NullableMetric {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl NullableMetric {
    pub fn some(value: Decimal) -> Self {
        Self { value: Some(value.round_dp(10).normalize().to_string()), reason: None }
    }

    pub fn none(reason: impl Into<String>) -> Self {
        Self { value: None, reason: Some(reason.into()) }
    }
}

/// 指标集合（契约 Comparison.metrics：收益、年化、回撤、波动、Sharpe、换手、成本）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MetricsSet {
    pub total_return: NullableMetric,
    pub annualized_return: NullableMetric,
    pub max_drawdown: NullableMetric,
    pub volatility: NullableMetric,
    pub sharpe: NullableMetric,
    pub turnover: NullableMetric,
    pub cost_cny: NullableMetric,
}

/// √252（年化因子，日频→年频）。
fn sqrt_252() -> Decimal {
    decimal_sqrt(Decimal::from(252))
}

/// 十进制平方根（Newton 迭代，确定性；结果 12 位小数精度）。
fn decimal_sqrt(v: Decimal) -> Decimal {
    if v <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    let mut x = if v >= Decimal::ONE { v } else { Decimal::ONE };
    for _ in 0..64 {
        let next = (x + v / x) / Decimal::from(2);
        let delta = (next - x).abs();
        x = next;
        if delta < Decimal::new(1, 12) {
            break;
        }
    }
    x.round_dp(12)
}

/// 逐日收益率序列：r_i = v_i / v_{i-1} - 1（前值为 0 时跳过该点，避免除零）。
pub fn daily_returns(values: &[Decimal]) -> Vec<Decimal> {
    let mut out = Vec::with_capacity(values.len().saturating_sub(1));
    for pair in values.windows(2) {
        let (prev, cur) = (pair[0], pair[1]);
        if prev.is_zero() {
            continue;
        }
        out.push(cur / prev - Decimal::ONE);
    }
    out
}

/// 最大回撤：max((峰值 - 值)/峰值)，按完整序列计算（UT-S14-01）。
pub fn max_drawdown(values: &[Decimal]) -> Decimal {
    let mut peak = Decimal::ZERO;
    let mut dd = Decimal::ZERO;
    for v in values {
        if *v > peak {
            peak = *v;
        }
        if peak > Decimal::ZERO {
            let d = (peak - v) / peak;
            if d > dd {
                dd = d;
            }
        }
    }
    dd
}

/// 样本标准差（n-1 分母）；样本 <2 返回 None。
fn sample_std(values: &[Decimal]) -> Option<Decimal> {
    let n = values.len();
    if n < 2 {
        return None;
    }
    let n_dec = Decimal::from(n as u64);
    let mean = values.iter().copied().sum::<Decimal>() / n_dec;
    let var = values
        .iter()
        .map(|r| {
            let d = *r - mean;
            d * d
        })
        .sum::<Decimal>()
        / (n_dec - Decimal::ONE);
    Some(decimal_sqrt(var))
}

/// 计算指标集合。
///
/// - `equity`：按交易日升序的净值序列（完整序列，不降采样）；
/// - `initial`：初始资金；
/// - `traded_value`：成交额合计（换手分子）；
/// - `fees`：费用合计。
///
/// 年化收益为首版线性近似（总收益 × 252 / 样本数），几何年化在指标版本化时替换。
pub fn compute_metrics(
    equity: &[Decimal],
    initial: Decimal,
    traded_value: Decimal,
    fees: Decimal,
) -> MetricsSet {
    if equity.is_empty() {
        return MetricsSet {
            total_return: NullableMetric::none("无净值样本"),
            annualized_return: NullableMetric::none("无净值样本"),
            max_drawdown: NullableMetric::none("无净值样本"),
            volatility: NullableMetric::none("无净值样本"),
            sharpe: NullableMetric::none("无净值样本"),
            turnover: NullableMetric::none("无净值样本"),
            cost_cny: NullableMetric::none("无净值样本"),
        };
    }
    let total_return = if initial > Decimal::ZERO {
        NullableMetric::some((*equity.last().unwrap() / initial) - Decimal::ONE)
    } else {
        NullableMetric::none("初始资金非正")
    };
    let annualized = match (&total_return.value, initial > Decimal::ZERO) {
        (Some(tr), true) => {
            let years = Decimal::from(equity.len() as u64);
            NullableMetric::some(
                tr.parse::<Decimal>().unwrap_or(Decimal::ZERO) * Decimal::from(252) / years,
            )
        }
        (Some(_), false) => NullableMetric::none("初始资金非正"),
        (None, _) => NullableMetric::none("无净值样本"),
    };
    let returns = daily_returns(equity);
    let std = sample_std(&returns);
    let volatility = match &std {
        Some(s) => NullableMetric::some(*s * sqrt_252()),
        None => NullableMetric::none("仅1个收益样本，波动不可计算"),
    };
    let sharpe = match (&std, returns.len()) {
        (Some(s), n) if *s > Decimal::ZERO && n >= 2 => {
            let mean = returns.iter().copied().sum::<Decimal>() / Decimal::from(n as u64);
            NullableMetric::some(mean / *s * sqrt_252())
        }
        (Some(_), n) if n < 2 => NullableMetric::none("仅1个收益样本，Sharpe不可计算"),
        (Some(_), _) => NullableMetric::none("波动为零，Sharpe不可计算"),
        (None, _) => NullableMetric::none("仅1个收益样本，Sharpe不可计算"),
    };
    let max_dd = NullableMetric::some(max_drawdown(equity));
    let turnover = if initial > Decimal::ZERO {
        NullableMetric::some(traded_value / initial)
    } else {
        NullableMetric::none("初始资金非正")
    };
    MetricsSet {
        total_return,
        annualized_return: annualized,
        max_drawdown: max_dd,
        volatility,
        sharpe,
        turnover,
        cost_cny: NullableMetric::some(fees),
    }
}
