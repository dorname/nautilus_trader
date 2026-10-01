//! 信号选股：top_k 槽位选择（S13-AC-01，UT-S13-15）。
//!
//! 语义：只为正分标的占槽；分数相等按标的代码升序稳定排序；
//! 正分不足 K 时未占槽位留现金，不向剩余标的自动加权。

use rust_decimal::Decimal;

/// 一个候选信号：标的 + 分数（None = 数据不足，不参与排序）。
#[derive(Debug, Clone, PartialEq)]
pub struct SignalScore {
    pub instrument_id: String,
    pub score: Option<Decimal>,
}

/// 选出 top_k 目标槽位：正分优先、分数降序、平分按代码升序。
/// 返回长度 ≤ top_k；不足部分由调用方留现金（equal_slots 权重）。
pub fn select_top_k(scores: &[SignalScore], top_k: usize) -> Vec<String> {
    let mut positive: Vec<&SignalScore> = scores
        .iter()
        .filter(|s| s.score.is_some_and(|v| v.is_sign_positive() && !v.is_zero()))
        .collect();
    positive.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.instrument_id.cmp(&b.instrument_id))
    });
    positive
        .into_iter()
        .take(top_k)
        .map(|s| s.instrument_id.clone())
        .collect()
}
