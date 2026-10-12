//! 文件日志（local-launch-logs-20261012）：stderr + `logs/` 目录双写。
//!
//! std-only 零新依赖（`--offline` verify 约束，不引入 tracing/chrono）。
//! 里程碑/桥接低频日志口径：每行 open-append 写文件，无全局锁状态。
//! 时间戳统一 UTC（std 无本地时区库；行内带 `UTC` 标注避免歧义）。

use std::path::{Path, PathBuf};

/// 指定日志目录的环境变量（优先于默认 `./logs`）。
pub const LOG_DIR_ENV: &str = "RESEARCH_DESKTOP_LOG_DIR";

/// 会话日志文件路径（`init_file_logger` 成功后缓存；未初始化时
/// `logline` 仅走 stderr，不影响既有行为）。
static SESSION_FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// 日志目录解析：`RESEARCH_DESKTOP_LOG_DIR` > 默认 `./logs`（相对 cwd）。
pub fn resolve_log_dir() -> PathBuf {
    std::env::var_os(LOG_DIR_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("logs"))
}

/// unix 秒 → (年, 月, 日, 时, 分, 秒) UTC。
/// Howard Hinnant civil_from_days 算法（公历，1970-01-01 起算）。
fn utc_from_unix(secs: u64) -> (u64, u8, u8, u8, u8, u8) {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (h, m, s) = (
        (rem / 3600) as u8,
        ((rem % 3600) / 60) as u8,
        (rem % 60) as u8,
    );
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let yr = if mo <= 2 { y + 1 } else { y };
    (yr, mo, d, h, m, s)
}

/// 当前 UTC 时间戳串：`YYYY-MM-DD HH:MM:SS.mmm`。
fn now_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (y, mo, d, h, mi, s) = utc_from_unix(now.as_secs());
    format!(
        "{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}.{:03}",
        now.subsec_millis()
    )
}

/// 创建会话日志文件（纯函数，UT 可重复调用）：建目录 +
/// `research-desktop-<YYYYMMDD-HHMMSS>.log`，返回文件路径。
pub fn create_session_file(dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (y, mo, d, h, mi, s) = utc_from_unix(now.as_secs());
    let file = dir.join(format!(
        "research-desktop-{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}.log"
    ));
    // 同秒重复启动（测试连发）时附加进程号避免互相覆盖
    let file = if file.exists() {
        dir.join(format!(
            "research-desktop-{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}-{}.log",
            std::process::id()
        ))
    } else {
        file
    };
    std::fs::File::create(&file)?;
    Ok(file)
}

/// 生产入口：按 [`resolve_log_dir`] 初始化会话文件（幂等，OnceLock 缓存）。
/// 返回会话文件路径；目录不可写时返回 None（`logline` 退化为仅 stderr）。
pub fn init_file_logger() -> Option<PathBuf> {
    let dir = resolve_log_dir();
    let path = create_session_file(&dir).ok();
    if let Some(p) = &path {
        // OnceLock 仅首次生效；重复 init（理论上不发生）不覆盖会话文件
        let _ = SESSION_FILE.set(p.clone());
    }
    path
}

/// 写一行日志到指定文件（纯函数核心，UT 直接验证落盘）：
/// `[YYYY-MM-DD HH:MM:SS.mmm UTC] msg` 追加写入。
pub fn logline_to(file: &Path, msg: &str) {
    let line = format!("[{} UTC] {msg}\n", now_stamp());
    if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(file) {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
    }
}

/// 统一日志入口：stderr（保持既有控制台可见性）+ 会话文件追加
///（未 init 时仅 stderr）。供 main / oup / swarm 桥接层统一调用。
pub fn logline(msg: &str) {
    eprintln!("[{} UTC] {msg}", now_stamp());
    if let Some(file) = SESSION_FILE.get() {
        logline_to(file, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S17-30（local-launch-logs-20261012）：init 创建目录与会话文件；
    /// logline 落盘内容含消息与时间戳（tmp 目录隔离）。
    #[test]
    fn ut_s17_30_file_logger_writes() {
        nautilus_research_testkit::case("UT-S17-30", || {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir().join(format!(
                "rd-logging-test-{}-{nanos}",
                std::process::id()
            ));

            // init：目录自动创建 + 会话文件生成且命名规范
            let file = create_session_file(&dir).expect("创建会话文件");
            assert!(file.exists(), "会话文件应存在");
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                name.starts_with("research-desktop-") && name.ends_with(".log"),
                "文件名规范：{name}"
            );

            // logline 落盘：含消息与 [HH:MM:SS 时间戳
            logline_to(&file, "首帧渲染到达（测试消息）");
            let text = std::fs::read_to_string(&file).expect("读日志");
            assert!(text.contains("首帧渲染到达（测试消息）"), "落盘应含消息");
            assert!(text.contains(" UTC] "), "落盘应含 UTC 时间戳：{text}");

            // 追加语义：第二行不覆盖第一行
            logline_to(&file, "第二条");
            let text2 = std::fs::read_to_string(&file).expect("再读");
            assert_eq!(text2.lines().count(), 2, "追加而非覆盖");

            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// 内部单测（非规格用例）：UTC 转换与目录解析纯函数。
    #[test]
    fn utc_conversion_epoch() {
        assert_eq!(utc_from_unix(0), (1970, 1, 1, 0, 0, 0));
        // 2026-10-12 00:00:00 UTC = 1791763200
        assert_eq!(utc_from_unix(1_791_763_200), (2026, 10, 12, 0, 0, 0));
    }
}
