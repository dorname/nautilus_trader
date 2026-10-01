//! OpenLogos reporter：所有研究 crate 测试共享的结果上报工具。
//!
//! 契约见 `logos/spec/test-results.md` 与 `logos/resources/test/core-09-research-test-cases.md`：
//! - 输出 `logos/resources/verify/test-results.jsonl`（JSONL，每行一个对象）；
//! - 字段：`id`、`status`、`duration_ms`、`timestamp`、`scenario`，失败必须含 `error`，
//!   另记录 `proposal`、`platform`、`run_id` 用于溯源；
//! - 整轮测试开始前由调用方（verify.pre_run_command）清空文件，各测试进程只追加。

use std::{
    fs::{self, OpenOptions},
    io::Write,
    panic::{catch_unwind, resume_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::OnceLock,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// 当前变更提案标识，可用环境变量覆盖。
fn proposal() -> String {
    std::env::var("OPENLOGOS_PROPOSAL").unwrap_or_else(|_| "astock-research-desktop".to_string())
}

/// 本轮运行标识：同一测试进程内稳定，进程间区分。
fn run_id() -> &'static str {
    static RUN_ID: OnceLock<String> = OnceLock::new();
    RUN_ID.get_or_init(|| {
        std::env::var("OPENLOGOS_RUN_ID").unwrap_or_else(|_| {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("pid-{}-{nanos}", std::process::id())
        })
    })
}

/// 定位仓库根（包含 `logos/resources` 的最近祖先目录）。
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .find(|p| p.join("logos/resources").is_dir())
        .unwrap_or_else(|| manifest.as_path())
        .to_path_buf()
}

/// 结果文件路径：`logos/resources/verify/test-results.jsonl`。
pub fn results_path() -> PathBuf {
    repo_root().join("logos/resources/verify/test-results.jsonl")
}

/// 从用例 ID（如 `UT-S11-01`）解析场景号（`S11`）。
fn scenario_of(case_id: &str) -> String {
    case_id
        .split('-')
        .nth(1)
        .filter(|s| s.starts_with('S'))
        .unwrap_or("UNKNOWN")
        .to_string()
}

fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::time::format_unix_rfc3339(secs)
}

/// 追加一条测试结果。首次写入前确保目录存在；绝不主动清空文件。
pub fn report_result(case_id: &str, status: &str, duration_ms: u128, error: Option<&str>) {
    let path = results_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let mut record = serde_json::json!({
        "id": case_id,
        "status": status,
        "duration_ms": duration_ms,
        "timestamp": now_rfc3339(),
        "scenario": scenario_of(case_id),
        "proposal": proposal(),
        "platform": std::env::consts::OS,
        "run_id": run_id(),
    });
    if let Some(err) = error {
        record["error"] = serde_json::Value::String(err.to_string());
    }
    // 单缓冲一次性写入：O_APPEND 下单次 write 原子，避免并发用例行交错
    let mut line = serde_json::to_string(&record).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// 执行一个测试用例并上报结果：通过记 `pass`，panic 记 `fail`（含信息）后重新抛出。
pub fn case<F>(case_id: &str, f: F)
where
    F: FnOnce() + std::panic::UnwindSafe,
{
    let start = Instant::now();
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(()) => report_result(case_id, "pass", start.elapsed().as_millis(), None),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
                .unwrap_or_else(|| "panic without message".to_string());
            report_result(case_id, "fail", start.elapsed().as_millis(), Some(&msg));
            resume_unwind(payload);
        }
    }
}

/// 本 crate 自用的最小 UTC 时间格式化（避免额外依赖）。
pub mod time {
    /// 将 Unix 秒格式化为 RFC3339（UTC，`YYYY-MM-DDTHH:MM:SSZ`）。
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

    /// Howard Hinnant 民用日期算法：Unix 日数转年月日。
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
}
