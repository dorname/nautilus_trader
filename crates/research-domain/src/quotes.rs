//! 日线行情行与暂存 CSV 解析校验。
//!
//! 校验规则（UT-S11-05）：非法日期、负成交量、high<low、重复键且无裁决来源
//! → 整批拒绝并返回行号；不允许部分完整快照。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    error::{ResearchError, Result},
    protocol::PriceBasis,
    time::parse_trade_date,
};

/// 规范化日线行情行（Parquet 分区内容）。
/// `amount_cny` 可为空：成交额缺失是真实数据形态（UT-S12-05），
/// 缺失不等于 0，也不允许用 close×volume 替代。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteRow {
    pub instrument_id: String,
    pub trade_date: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume_shares: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_cny: Option<Decimal>,
}

/// 暂存 CSV 表头（Python 数据桥接的协议化输出；测试直接用同格式文件）。
pub const STAGING_CSV_HEADER: &str =
    "instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny";

/// 单行解析错误：行号为 1 起始的数据行号（不含表头）。
#[derive(Debug, Clone)]
pub struct RowError {
    pub line: usize,
    pub reason: String,
}

/// 解析并校验暂存 CSV；任何非法行使整批被拒绝。
pub fn parse_staging_csv(text: &str) -> std::result::Result<Vec<QuoteRow>, Vec<RowError>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();

    let header = lines.next().unwrap_or("");
    if header.trim().trim_start_matches('\u{feff}') != STAGING_CSV_HEADER {
        errors.push(RowError {
            line: 0,
            reason: format!("表头不符，期望：{STAGING_CSV_HEADER}"),
        });
        return Err(errors);
    }

    for (idx, raw) in lines.enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            continue;
        }
        match parse_row(raw, line_no) {
            Ok(row) => rows.push(row),
            Err(e) => errors.push(e),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(rows)
}

fn parse_row(raw: &str, line_no: usize) -> std::result::Result<QuoteRow, RowError> {
    let err = |reason: &str| RowError {
        line: line_no,
        reason: reason.to_string(),
    };
    let cols: Vec<&str> = raw.split(',').map(str::trim).collect();
    if cols.len() != 8 {
        return Err(err(&format!("列数 {} 不等于 8", cols.len())));
    }
    let instrument_id = cols[0].to_string();
    if instrument_id.is_empty() {
        return Err(err("标的代码为空"));
    }
    let trade_date = cols[1].to_string();
    if parse_trade_date(&trade_date).is_err() {
        return Err(err(&format!("非法日期：{trade_date}")));
    }
    let dec = |s: &str, name: &str| -> std::result::Result<Decimal, RowError> {
        Decimal::from_str_exact(s).map_err(|_| err(&format!("{name} 不是十进制数：{s}")))
    };
    let open = dec(cols[2], "open")?;
    let high = dec(cols[3], "high")?;
    let low = dec(cols[4], "low")?;
    let close = dec(cols[5], "close")?;
    if high < low {
        return Err(err(&format!("最高价 {high} 小于最低价 {low}")));
    }
    let volume_shares: u64 = cols[6]
        .parse()
        .map_err(|_| err(&format!("成交量不是非负整数：{}", cols[6])))?;
    // 成交额允许为空（缺失）；非空时必须是非负十进制数
    let amount_cny = if cols[7].is_empty() {
        None
    } else {
        let a = dec(cols[7], "amount_cny")?;
        if a.is_sign_negative() {
            return Err(err(&format!("成交额不能为负：{a}")));
        }
        Some(a)
    };
    Ok(QuoteRow {
        instrument_id,
        trade_date,
        open,
        high,
        low,
        close,
        volume_shares,
        amount_cny,
    })
}

/// 键级冲突检查：同一标的/交易日出现多行即冲突；无裁决来源时整批拒绝。
pub fn reject_duplicate_keys(rows: &[QuoteRow]) -> Result<()> {
    let mut seen = std::collections::HashMap::<(&str, &str), usize>::new();
    for (i, r) in rows.iter().enumerate() {
        let key = (r.instrument_id.as_str(), r.trade_date.as_str());
        if let Some(first) = seen.insert(key, i + 1) {
            return Err(ResearchError::invalid(format!(
                "重复行情键且无裁决来源：{} {}（第 {} 行与第 {} 行）",
                r.instrument_id,
                r.trade_date,
                first,
                i + 1
            ))
            .with_field("rows"));
        }
    }
    Ok(())
}

/// 应用可选的标的与日期区间过滤（ImportSpec.symbols / start / end）。
pub fn apply_scope(rows: Vec<QuoteRow>, spec: &crate::protocol::ImportSpec) -> Vec<QuoteRow> {
    rows.into_iter()
        .filter(|r| {
            spec.symbols
                .as_ref()
                .is_none_or(|ss| ss.iter().any(|s| s == &r.instrument_id))
        })
        .filter(|r| spec.start.as_ref().is_none_or(|s| r.trade_date >= *s))
        .filter(|r| spec.end.as_ref().is_none_or(|e| r.trade_date <= *e))
        .collect()
}

/// 分区键：按标的与价格口径分区；raw/qfq/hfq 禁止混入同一分区。
pub fn partition_name(instrument_id: &str, basis: PriceBasis) -> String {
    format!("quotes-{instrument_id}-{}.parquet", basis.as_str())
}
