//! 研究信号指标：十进制定点实现，确定性、无浮点漂移（S13-AC-01）。
//!
//! 规格数值（UT-S13-01）：
//! - EMA 以首个值播种：收盘 10、11，快 1 → 11；慢 2 → 10 + (11-10)×2/3 = 10.666…
//! - 动量 score = close[t-skip] / close[t-skip-lookback] - 1：
//!   10/12/15/18，L=2、s=1 → 15/10-1 = 0.5（不能用最新的 18）。

use rust_decimal::Decimal;

use crate::error::{ResearchError, Result};

/// EMA 末值：以首个观测值播种，alpha = 2/(period+1)。
/// 序列不足 period 个观测返回 None（预热不足，不伪造）。
pub fn ema_last(closes: &[Decimal], period: u32) -> Option<Decimal> {
    if period == 0 || closes.len() < period as usize {
        return None;
    }
    let alpha = Decimal::from(2u64) / Decimal::from(u64::from(period) + 1);
    let mut ema = *closes.first()?;
    for c in &closes[1..] {
        ema = *c * alpha + ema * (Decimal::ONE - alpha);
    }
    Some(ema)
}

/// 动量分数：close[t-skip]/close[t-skip-lookback] - 1。
/// 历史不足（含 skip）返回 None。
pub fn momentum_score(closes: &[Decimal], lookback: u32, skip: u32) -> Option<Decimal> {
    let n = closes.len();
    let need = (lookback + skip + 1) as usize;
    if lookback == 0 || n < need {
        return None;
    }
    let recent = closes[n - 1 - skip as usize];
    let base = closes[n - 1 - (skip + lookback) as usize];
    if base.is_zero() {
        return None;
    }
    Some(recent / base - Decimal::ONE)
}

/// 参数校验（StrategySpec 约束）：ema 要求 1≤fast<slow≤2500；momentum 要求
/// 1≤lookback≤2500、0≤skip≤250。
pub fn validate_indicator_params(template: &str, params: &IndicatorParams) -> Result<()> {
    match (template, params) {
        ("ema", IndicatorParams::Ema { fast, slow }) => {
            if *fast < 1 || *slow > 2500 || fast >= slow {
                return Err(ResearchError::invalid("ema 要求 1≤fast<slow≤2500").with_field("strategy.fast"));
            }
            Ok(())
        }
        ("momentum", IndicatorParams::Momentum { lookback, skip }) => {
            if !(1..=2500).contains(lookback) {
                return Err(ResearchError::invalid("momentum 要求 1≤lookback≤2500").with_field("strategy.lookback"));
            }
            if *skip > 250 {
                return Err(ResearchError::invalid("momentum 要求 0≤skip≤250").with_field("strategy.skip"));
            }
            Ok(())
        }
        ("ema", _) => Err(ResearchError::invalid("ema 模板需要 fast/slow 参数").with_field("strategy")),
        ("momentum", _) => Err(ResearchError::invalid("momentum 模板需要 lookback/skip 参数").with_field("strategy")),
        _ => Err(ResearchError::invalid(format!("未知策略模板：{template}")).with_field("strategy.template")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IndicatorParams {
    Ema { fast: u32, slow: u32 },
    Momentum { lookback: u32, skip: u32 },
}
