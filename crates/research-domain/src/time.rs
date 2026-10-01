//! 最小 UTC 时间工具：RFC3339 格式化与交易日（上海）字符串校验。
//! 不引入 chrono，避免额外依赖。

use crate::error::{ResearchError, Result};

/// Unix 秒 → RFC3339（UTC）。
pub fn format_unix_rfc3339(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 当前 UTC 时间（RFC3339）。
pub fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_rfc3339(secs)
}

/// Howard Hinnant 民用日期算法：Unix 日数 → (年, 月, 日)。
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (年, 月, 日) → Unix 日数。
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + u64::from(d - 1);
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe as i64 - 719_468
}

/// 校验并规范化交易日字符串 `YYYY-MM-DD`（Asia/Shanghai 日历日）。
pub fn parse_trade_date(s: &str) -> Result<(i64, u32, u32)> {
    let bad = || ResearchError::invalid(format!("非法交易日：{s}，期望 YYYY-MM-DD")).with_field("trade_date");
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(bad());
    }
    let (y, m, d) = (
        parts[0].parse::<i64>().map_err(|_| bad())?,
        parts[1].parse::<u32>().map_err(|_| bad())?,
        parts[2].parse::<u32>().map_err(|_| bad())?,
    );
    if parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err(bad());
    }
    if !(1..=12).contains(&m) {
        return Err(bad());
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize];
    if d == 0 || d > dim {
        return Err(bad());
    }
    Ok((y, m, d))
}

/// 交易日字符串 → Unix 日数（Date32 用）。
pub fn trade_date_to_date32(s: &str) -> Result<i32> {
    let (y, m, d) = parse_trade_date(s)?;
    Ok(days_from_civil(y, m, d) as i32)
}

/// Unix 日数 → 交易日字符串。
pub fn date32_to_trade_date(days: i32) -> String {
    let (y, m, d) = civil_from_days(i64::from(days));
    format!("{y:04}-{m:02}-{d:02}")
}
