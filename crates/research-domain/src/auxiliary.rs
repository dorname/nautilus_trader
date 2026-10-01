//! 辅助数据（证券主档、财务报告）暂存解析与规范化。
//!
//! 架构「数据模型与补齐入口」节：主档含上市/退市日期与板块；财务表含
//! period_end/available_at/修订号/数值。公告时间（available_at）是时点正确性的
//! 前提，缺失即整批拒绝；数值字段允许为空（缺失 ≠ 0）。
//! 辅助数据量小且需内容寻址，落库形式为规范化 JSON 内容对象（非 Parquet）。

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{
    quotes::RowError,
    time::parse_trade_date,
};

/// 证券主档记录：板块、上市日、退市日（空 = 未退市）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterRecord {
    pub instrument_id: String,
    pub board: String,
    pub listed_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delisted_date: Option<String>,
}

/// 财务报告记录：period_end 为报告期，available_at 为公告日（时点可见性），
/// revision 为修订号（同报告期取最大）；指标为空表示该报告期未披露。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinancialRecord {
    pub instrument_id: String,
    pub period_end: String,
    pub available_at: String,
    pub revision: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roe: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eps: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bps: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub net_profit: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revenue_growth: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debt_ratio: Option<Decimal>,
}

pub const MASTER_CSV_HEADER: &str = "instrument_id,board,listed_date,delisted_date";
pub const FINANCIAL_CSV_HEADER: &str =
    "instrument_id,period_end,available_at,revision,roe,eps,bps,net_profit,revenue_growth,debt_ratio";

/// 辅助数据种类（契约 ImportSpec.auxiliary_kind；本批实现 master/financial）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuxKind {
    Master,
    Financial,
}

impl AuxKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Financial => "financial",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "master" => Some(Self::Master),
            "financial" => Some(Self::Financial),
            _ => None,
        }
    }
}

/// 按表头识别暂存文件种类（辅助文件与行情文件表头互不重叠）。
pub fn sniff_kind(text: &str) -> Option<AuxKind> {
    let header = text.lines().next().unwrap_or("").trim().trim_start_matches('\u{feff}');
    match header {
        MASTER_CSV_HEADER => Some(AuxKind::Master),
        FINANCIAL_CSV_HEADER => Some(AuxKind::Financial),
        _ => None,
    }
}

/// 解析主档暂存 CSV；任何非法行使整批被拒绝。
pub fn parse_master_csv(text: &str) -> std::result::Result<Vec<MasterRecord>, Vec<RowError>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header.trim().trim_start_matches('\u{feff}') != MASTER_CSV_HEADER {
        return Err(vec![RowError {
            line: 0,
            reason: format!("表头不符，期望：{MASTER_CSV_HEADER}"),
        }]);
    }
    for (idx, raw) in lines.enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let err = |reason: &str| RowError { line: line_no, reason: reason.to_string() };
        let cols: Vec<&str> = raw.split(',').map(str::trim).collect();
        if cols.len() != 4 {
            errors.push(err(&format!("列数 {} 不等于 4", cols.len())));
            continue;
        }
        if cols[0].is_empty() {
            errors.push(err("标的代码为空"));
            continue;
        }
        if cols[1].is_empty() {
            errors.push(err("板块为空"));
            continue;
        }
        if parse_trade_date(cols[2]).is_err() {
            errors.push(err(&format!("非法上市日期：{}", cols[2])));
            continue;
        }
        let delisted = if cols[3].is_empty() {
            None
        } else if parse_trade_date(cols[3]).is_ok() {
            Some(cols[3].to_string())
        } else {
            errors.push(err(&format!("非法退市日期：{}", cols[3])));
            continue;
        };
        if let Some(d) = &delisted {
            if d.as_str() < cols[2] {
                errors.push(err(&format!("退市日期 {d} 早于上市日期 {}", cols[2])));
                continue;
            }
        }
        rows.push(MasterRecord {
            instrument_id: cols[0].to_string(),
            board: cols[1].to_string(),
            listed_date: cols[2].to_string(),
            delisted_date: delisted,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // 主档主键唯一：同一标的重复即冲突
    let mut seen = std::collections::HashSet::new();
    for (i, r) in rows.iter().enumerate() {
        if !seen.insert(r.instrument_id.as_str()) {
            return Err(vec![RowError {
                line: i + 1,
                reason: format!("主档标的重复：{}", r.instrument_id),
            }]);
        }
    }
    Ok(rows)
}

/// 解析财务暂存 CSV；任何非法行使整批被拒绝。
/// available_at 必须非空（公告时间缺失无法保证时点正确性）。
pub fn parse_financial_csv(text: &str) -> std::result::Result<Vec<FinancialRecord>, Vec<RowError>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header.trim().trim_start_matches('\u{feff}') != FINANCIAL_CSV_HEADER {
        return Err(vec![RowError {
            line: 0,
            reason: format!("表头不符，期望：{FINANCIAL_CSV_HEADER}"),
        }]);
    }
    for (idx, raw) in lines.enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let err = |reason: &str| RowError { line: line_no, reason: reason.to_string() };
        let cols: Vec<&str> = raw.split(',').map(str::trim).collect();
        if cols.len() != 10 {
            errors.push(err(&format!("列数 {} 不等于 10", cols.len())));
            continue;
        }
        if cols[0].is_empty() {
            errors.push(err("标的代码为空"));
            continue;
        }
        if parse_trade_date(cols[1]).is_err() {
            errors.push(err(&format!("非法报告期：{}", cols[1])));
            continue;
        }
        if cols[2].is_empty() {
            errors.push(err("公告时间 available_at 缺失，整批拒绝"));
            continue;
        }
        if parse_trade_date(cols[2]).is_err() {
            errors.push(err(&format!("非法公告时间：{}", cols[2])));
            continue;
        }
        let revision: u32 = match cols[3].parse() {
            Ok(r) => r,
            Err(_) => {
                errors.push(err(&format!("修订号不是非负整数：{}", cols[3])));
                continue;
            }
        };
        let dec = |s: &str, name: &str| -> std::result::Result<Option<Decimal>, RowError> {
            if s.is_empty() {
                return Ok(None);
            }
            Decimal::from_str_exact(s)
                .map(Some)
                .map_err(|_| err(&format!("{name} 不是十进制数：{s}")))
        };
        let parsed = (
            dec(cols[4], "roe"),
            dec(cols[5], "eps"),
            dec(cols[6], "bps"),
            dec(cols[7], "net_profit"),
            dec(cols[8], "revenue_growth"),
            dec(cols[9], "debt_ratio"),
        );
        match parsed {
            (Ok(roe), Ok(eps), Ok(bps), Ok(np), Ok(rg), Ok(dr)) => rows.push(FinancialRecord {
                instrument_id: cols[0].to_string(),
                period_end: cols[1].to_string(),
                available_at: cols[2].to_string(),
                revision,
                roe,
                eps,
                bps,
                net_profit: np,
                revenue_growth: rg,
                debt_ratio: dr,
            }),
            (r0, r1, r2, r3, r4, r5) => {
                for e in [r0, r1, r2, r3, r4, r5].into_iter().filter_map(|r| r.err()) {
                    errors.push(e);
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // 财务主键：（标的、报告期、修订号）唯一
    let mut seen = std::collections::HashSet::new();
    for (i, r) in rows.iter().enumerate() {
        let key = (r.instrument_id.as_str(), r.period_end.as_str(), r.revision);
        if !seen.insert(key) {
            return Err(vec![RowError {
                line: i + 1,
                reason: format!(
                    "财务记录重复：{} {} 修订号 {}",
                    r.instrument_id, r.period_end, r.revision
                ),
            }]);
        }
    }
    Ok(rows)
}

/// 主档内容对象字节：按标的排序的规范化 JSON（确定性哈希）。
pub fn master_json_bytes(rows: &[MasterRecord]) -> Vec<u8> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| a.instrument_id.cmp(&b.instrument_id));
    #[derive(Serialize)]
    struct MasterDoc<'a> {
        kind: &'static str,
        rows: &'a [MasterRecord],
    }
    crate::hash::canonical_json(&MasterDoc { kind: "master", rows: &rows }).into_bytes()
}

/// 财务内容对象字节：按（标的、报告期、修订号）排序的规范化 JSON。
pub fn financial_json_bytes(rows: &[FinancialRecord]) -> Vec<u8> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| {
        (&a.instrument_id, &a.period_end, a.revision).cmp(&(&b.instrument_id, &b.period_end, b.revision))
    });
    #[derive(Serialize)]
    struct FinDoc<'a> {
        kind: &'static str,
        rows: &'a [FinancialRecord],
    }
    crate::hash::canonical_json(&FinDoc { kind: "financial", rows: &rows }).into_bytes()
}

/// 读取主档内容对象。
pub fn master_from_bytes(bytes: &[u8]) -> crate::error::Result<Vec<MasterRecord>> {
    #[derive(Deserialize)]
    struct MasterDoc {
        kind: String,
        rows: Vec<MasterRecord>,
    }
    let doc: MasterDoc = serde_json::from_slice(bytes).map_err(|e| {
        crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("主档内容对象解析失败：{e}"),
        )
    })?;
    if doc.kind != "master" {
        return Err(crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("内容对象种类不符：{}", doc.kind),
        ));
    }
    Ok(doc.rows)
}

/// 读取财务内容对象。
pub fn financial_from_bytes(bytes: &[u8]) -> crate::error::Result<Vec<FinancialRecord>> {
    #[derive(Deserialize)]
    struct FinDoc {
        kind: String,
        rows: Vec<FinancialRecord>,
    }
    let doc: FinDoc = serde_json::from_slice(bytes).map_err(|e| {
        crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("财务内容对象解析失败：{e}"),
        )
    })?;
    if doc.kind != "financial" {
        return Err(crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("内容对象种类不符：{}", doc.kind),
        ));
    }
    Ok(doc.rows)
}
