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
pub const ACTIONS_CSV_HEADER: &str =
    "instrument_id,announced_at,ex_date,pay_date,split_ratio,cash_per_share";
pub const CALENDAR_CSV_HEADER: &str = "trade_date";
pub const RULES_CSV_HEADER: &str =
    "market,board,effective_start,effective_end,tick,min_qty,qty_step,limit_pct";

/// 公司行为记录：拆股比例与每股现金分红（架构「数据模型与补齐入口」）。
/// announced_at 为公告时间（时点可见性）；pay_date 空表示无现金支付环节。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRecord {
    pub instrument_id: String,
    pub announced_at: String,
    pub ex_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pay_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_ratio: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cash_per_share: Option<Decimal>,
}

/// 交易日历记录：仅列出交易日（日历不靠工作日猜测）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarRecord {
    pub trade_date: String,
}

/// 规则包记录：按板块与生效区间解析交易规则（架构「撮合接入与规则版本」）。
/// effective_end 空表示开放区间（现行有效）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleRecord {
    pub market: String,
    pub board: String,
    pub effective_start: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_end: Option<String>,
    pub tick: Decimal,
    pub min_qty: u64,
    pub qty_step: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_pct: Option<Decimal>,
}

/// 辅助数据种类（契约 ImportSpec.auxiliary_kind）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuxKind {
    Master,
    Financial,
    Actions,
    Calendar,
    Rules,
}

impl AuxKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Financial => "financial",
            Self::Actions => "actions",
            Self::Calendar => "calendar",
            Self::Rules => "rules",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "master" => Some(Self::Master),
            "financial" => Some(Self::Financial),
            "actions" => Some(Self::Actions),
            "calendar" => Some(Self::Calendar),
            "rules" => Some(Self::Rules),
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
        ACTIONS_CSV_HEADER => Some(AuxKind::Actions),
        CALENDAR_CSV_HEADER => Some(AuxKind::Calendar),
        RULES_CSV_HEADER => Some(AuxKind::Rules),
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

// ---------------------------------------------------------------- actions / calendar / rules

/// 解析公司行为暂存 CSV；公告时间缺失整批拒绝（时点正确性前提）。
pub fn parse_actions_csv(text: &str) -> std::result::Result<Vec<ActionRecord>, Vec<RowError>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header.trim().trim_start_matches('\u{feff}') != ACTIONS_CSV_HEADER {
        return Err(vec![RowError {
            line: 0,
            reason: format!("表头不符，期望：{ACTIONS_CSV_HEADER}"),
        }]);
    }
    for (idx, raw) in lines.enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let err = |reason: &str| RowError { line: line_no, reason: reason.to_string() };
        let cols: Vec<&str> = raw.split(',').map(str::trim).collect();
        if cols.len() != 6 {
            errors.push(err(&format!("列数 {} 不等于 6", cols.len())));
            continue;
        }
        if cols[0].is_empty() {
            errors.push(err("标的代码为空"));
            continue;
        }
        if cols[1].is_empty() || parse_trade_date(cols[1]).is_err() {
            errors.push(err("公告时间 announced_at 缺失或非法，整批拒绝"));
            continue;
        }
        if parse_trade_date(cols[2]).is_err() {
            errors.push(err(&format!("非法除权除息日：{}", cols[2])));
            continue;
        }
        let pay_date = if cols[3].is_empty() {
            None
        } else if parse_trade_date(cols[3]).is_ok() {
            Some(cols[3].to_string())
        } else {
            errors.push(err(&format!("非法支付日：{}", cols[3])));
            continue;
        };
        let dec = |s: &str, name: &str| -> std::result::Result<Option<Decimal>, RowError> {
            if s.is_empty() {
                return Ok(None);
            }
            Decimal::from_str_exact(s)
                .map(Some)
                .map_err(|_| err(&format!("{name} 不是十进制数：{s}")))
        };
        let (split, cash) = match (dec(cols[4], "split_ratio"), dec(cols[5], "cash_per_share")) {
            (Ok(s), Ok(c)) => (s, c),
            (Err(e), _) | (_, Err(e)) => {
                errors.push(e);
                continue;
            }
        };
        if split.is_none() && cash.is_none() {
            errors.push(err("拆股比例与每股现金至少要有一项"));
            continue;
        }
        if let Some(s) = split {
            if s <= Decimal::ZERO {
                errors.push(err("拆股比例必须为正"));
                continue;
            }
        }
        rows.push(ActionRecord {
            instrument_id: cols[0].to_string(),
            announced_at: cols[1].to_string(),
            ex_date: cols[2].to_string(),
            pay_date,
            split_ratio: split,
            cash_per_share: cash,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // 主键：（标的、除权除息日）唯一——同日多条合并型事件须在源数据合并
    let mut seen = std::collections::HashSet::new();
    for (i, r) in rows.iter().enumerate() {
        if !seen.insert((r.instrument_id.as_str(), r.ex_date.as_str())) {
            return Err(vec![RowError {
                line: i + 1,
                reason: format!("公司行为重复：{} {}", r.instrument_id, r.ex_date),
            }]);
        }
    }
    Ok(rows)
}

/// 解析交易日历暂存 CSV（每行一个交易日，升序由提交时排序保证）。
pub fn parse_calendar_csv(text: &str) -> std::result::Result<Vec<CalendarRecord>, Vec<RowError>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header.trim().trim_start_matches('\u{feff}') != CALENDAR_CSV_HEADER {
        return Err(vec![RowError {
            line: 0,
            reason: format!("表头不符，期望：{CALENDAR_CSV_HEADER}"),
        }]);
    }
    for (idx, raw) in lines.enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = raw.split(',').map(str::trim).collect();
        if cols.len() != 1 || parse_trade_date(cols[0]).is_err() {
            errors.push(RowError {
                line: line_no,
                reason: format!("非法交易日行：{raw}"),
            });
            continue;
        }
        rows.push(CalendarRecord { trade_date: cols[0].to_string() });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut seen = std::collections::HashSet::new();
    for (i, r) in rows.iter().enumerate() {
        if !seen.insert(r.trade_date.as_str()) {
            return Err(vec![RowError {
                line: i + 1,
                reason: format!("交易日重复：{}", r.trade_date),
            }]);
        }
    }
    Ok(rows)
}

/// 解析规则包暂存 CSV；生效区间必须完整（start 必填、end 可空=开放区间）。
pub fn parse_rules_csv(text: &str) -> std::result::Result<Vec<RuleRecord>, Vec<RowError>> {
    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut lines = text.lines();
    let header = lines.next().unwrap_or("");
    if header.trim().trim_start_matches('\u{feff}') != RULES_CSV_HEADER {
        return Err(vec![RowError {
            line: 0,
            reason: format!("表头不符，期望：{RULES_CSV_HEADER}"),
        }]);
    }
    for (idx, raw) in lines.enumerate() {
        let line_no = idx + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let err = |reason: &str| RowError { line: line_no, reason: reason.to_string() };
        let cols: Vec<&str> = raw.split(',').map(str::trim).collect();
        if cols.len() != 8 {
            errors.push(err(&format!("列数 {} 不等于 8", cols.len())));
            continue;
        }
        if cols[0].is_empty() || cols[1].is_empty() {
            errors.push(err("市场与板块不能为空"));
            continue;
        }
        if parse_trade_date(cols[2]).is_err() {
            errors.push(err(&format!("非法生效开始日：{}", cols[2])));
            continue;
        }
        let end = if cols[3].is_empty() {
            None
        } else if parse_trade_date(cols[3]).is_ok() {
            Some(cols[3].to_string())
        } else {
            errors.push(err(&format!("非法生效结束日：{}", cols[3])));
            continue;
        };
        if let Some(e) = &end {
            if e.as_str() < cols[2] {
                errors.push(err(&format!("生效区间倒置：{e} < {}", cols[2])));
                continue;
            }
        }
        let tick = match Decimal::from_str_exact(cols[4]) {
            Ok(t) if t > Decimal::ZERO => t,
            _ => {
                errors.push(err(&format!("tick 必须为正十进制数：{}", cols[4])));
                continue;
            }
        };
        let min_qty: u64 = match cols[5].parse() {
            Ok(v) => v,
            Err(_) => {
                errors.push(err(&format!("min_qty 不是非负整数：{}", cols[5])));
                continue;
            }
        };
        let qty_step: u64 = match cols[6].parse() {
            Ok(v) if v > 0 => v,
            _ => {
                errors.push(err(&format!("qty_step 必须为正整数：{}", cols[6])));
                continue;
            }
        };
        let limit_pct = if cols[7].is_empty() {
            None
        } else {
            match Decimal::from_str_exact(cols[7]) {
                Ok(v) if v > Decimal::ZERO => Some(v),
                _ => {
                    errors.push(err(&format!("limit_pct 必须为正十进制数：{}", cols[7])));
                    continue;
                }
            }
        };
        rows.push(RuleRecord {
            market: cols[0].to_string(),
            board: cols[1].to_string(),
            effective_start: cols[2].to_string(),
            effective_end: end,
            tick,
            min_qty,
            qty_step,
            limit_pct,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // 主键：（市场、板块、生效开始）唯一
    let mut seen = std::collections::HashSet::new();
    for (i, r) in rows.iter().enumerate() {
        if !seen.insert((r.market.as_str(), r.board.as_str(), r.effective_start.as_str())) {
            return Err(vec![RowError {
                line: i + 1,
                reason: format!(
                    "规则记录重复：{}/{} 生效 {}",
                    r.market, r.board, r.effective_start
                ),
            }]);
        }
    }
    Ok(rows)
}

/// 公司行为内容对象字节：按（标的、除权除息日）排序的规范化 JSON。
pub fn actions_json_bytes(rows: &[ActionRecord]) -> Vec<u8> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| {
        (&a.instrument_id, &a.ex_date).cmp(&(&b.instrument_id, &b.ex_date))
    });
    #[derive(Serialize)]
    struct Doc<'a> {
        kind: &'static str,
        rows: &'a [ActionRecord],
    }
    crate::hash::canonical_json(&Doc { kind: "actions", rows: &rows }).into_bytes()
}

/// 交易日历内容对象字节：升序规范化 JSON。
pub fn calendar_json_bytes(rows: &[CalendarRecord]) -> Vec<u8> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| a.trade_date.cmp(&b.trade_date));
    #[derive(Serialize)]
    struct Doc<'a> {
        kind: &'static str,
        rows: &'a [CalendarRecord],
    }
    crate::hash::canonical_json(&Doc { kind: "calendar", rows: &rows }).into_bytes()
}

/// 规则包内容对象字节：按（市场、板块、生效开始）排序的规范化 JSON。
pub fn rules_json_bytes(rows: &[RuleRecord]) -> Vec<u8> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| {
        (&a.market, &a.board, &a.effective_start).cmp(&(&b.market, &b.board, &b.effective_start))
    });
    #[derive(Serialize)]
    struct Doc<'a> {
        kind: &'static str,
        rows: &'a [RuleRecord],
    }
    crate::hash::canonical_json(&Doc { kind: "rules", rows: &rows }).into_bytes()
}

/// 读取公司行为内容对象。
pub fn actions_from_bytes(bytes: &[u8]) -> crate::error::Result<Vec<ActionRecord>> {
    #[derive(Deserialize)]
    struct Doc {
        kind: String,
        rows: Vec<ActionRecord>,
    }
    let doc: Doc = serde_json::from_slice(bytes).map_err(|e| {
        crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("公司行为内容对象解析失败：{e}"),
        )
    })?;
    if doc.kind != "actions" {
        return Err(crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("内容对象种类不符：{}", doc.kind),
        ));
    }
    Ok(doc.rows)
}

/// 读取交易日历内容对象。
pub fn calendar_from_bytes(bytes: &[u8]) -> crate::error::Result<Vec<CalendarRecord>> {
    #[derive(Deserialize)]
    struct Doc {
        kind: String,
        rows: Vec<CalendarRecord>,
    }
    let doc: Doc = serde_json::from_slice(bytes).map_err(|e| {
        crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("交易日历内容对象解析失败：{e}"),
        )
    })?;
    if doc.kind != "calendar" {
        return Err(crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("内容对象种类不符：{}", doc.kind),
        ));
    }
    Ok(doc.rows)
}

/// 读取规则包内容对象。
pub fn rules_from_bytes(bytes: &[u8]) -> crate::error::Result<Vec<RuleRecord>> {
    #[derive(Deserialize)]
    struct Doc {
        kind: String,
        rows: Vec<RuleRecord>,
    }
    let doc: Doc = serde_json::from_slice(bytes).map_err(|e| {
        crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("规则包内容对象解析失败：{e}"),
        )
    })?;
    if doc.kind != "rules" {
        return Err(crate::error::ResearchError::new(
            crate::error::ErrorCode::CorruptArtifact,
            format!("内容对象种类不符：{}", doc.kind),
        ));
    }
    Ok(doc.rows)
}
