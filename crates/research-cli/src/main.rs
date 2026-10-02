//! `research` 命令行入口：参数透传给库入口 `run_cli`，按返回码退出。

fn main() {
    let code = nautilus_research_cli::run_cli(
        std::env::args(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    std::process::exit(code);
}
