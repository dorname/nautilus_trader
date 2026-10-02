//! 研究桌面 CLI 库入口：`run_cli` 接收参数与输出通道，返回进程退出码。
//!
//! 库形态供 UT/ST 直接以进程内方式驱动全链路（ST-S21-01/02），
//! bin 入口（main.rs）只是 `std::env::args()` 的薄包装。

pub mod args;
pub mod commands;
pub mod output;

use std::io::Write;

use nautilus_research_domain::ErrorCode;

/// 用法错误（非零退出码 + 中文诊断到 stderr）。
fn fail(stderr: &mut impl Write, message: &str) -> i32 {
    let _ = writeln!(stderr, "{message}");
    args::EXIT_REJECTED
}

/// 退出码映射：可重试/环境类错误 → 4；其余契约错误 → 3。
fn exit_code_of(err: &nautilus_research_domain::ResearchError) -> i32 {
    match err.code {
        ErrorCode::Busy | ErrorCode::DiskFull | ErrorCode::WorkerExited => args::EXIT_ENV,
        _ => args::EXIT_REJECTED,
    }
}

fn error_rows(err: &nautilus_research_domain::ResearchError) -> Vec<(&'static str, String)> {
    let field = err.field.clone().unwrap_or_else(|| "-".to_string());
    vec![
        // 错误码以契约原文（SCREAMING）透传，与 JSON 错误体口径一致
        ("错误码", code_as_str(&err.code)),
        ("字段", field),
        ("原因", err.message.clone()),
        ("可重试", if err.retryable { "是" } else { "否" }.to_string()),
    ]
}

/// 错误码契约原文（SCREAMING；经 serde rename，与错误体 JSON 同源）。
fn code_as_str(code: &ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{code:?}"))
}

/// 执行一次 CLI 调用；`argv[0]` 为程序名（跳过）。
pub fn run_cli<I, W, E>(argv: I, stdout: &mut W, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = String>,
    W: Write,
    E: Write,
{
    let v: Vec<String> = argv.into_iter().collect();
    let inv = match args::parse(&v) {
        Ok(inv) => inv,
        Err(msg) => return fail(stderr, &msg),
    };
    match commands::dispatch(inv, stdout) {
        Ok(code) => code,
        Err(err) => {
            let _ = writeln!(stderr, "命令失败：{}", err.message);
            let _ = output::emit_kv(stderr, &error_rows(&err));
            exit_code_of(&err)
        }
    }
}
