//! 研究运行工作进程入口：`research-run-task <workspace> <config_hash>`。
//!
//! 凭证红线：由协调器以清空后的环境启动，不继承 TICKFLOW_API_KEY；
//! 日志只写 stderr，stdout 预留给协议通道。退出码契约见
//! nautilus_research_domain::executor。

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("用法：research-run-task <workspace> <config_hash>");
        std::process::exit(2);
    }
    let workspace = std::path::PathBuf::from(&args[1]);
    let config_hash = &args[2];
    let code = nautilus_research_worker::adapter::run_task(&workspace, config_hash);
    std::process::exit(code);
}
