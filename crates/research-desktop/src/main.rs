//! 研究桌面原生入口：黑色玻璃态窗口（默认 1440×900，最小 1100×720）。
//!
//! 无可用图形后端时以中文提示失败退出（退出码 4，与 CLI 环境错误对齐），
//! 不把软件渲染支持当作已验证承诺（core-03-desktop-delivery）。

use nautilus_research_desktop::{app::ResearchApp, theme};

fn main() {
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(theme::DEFAULT_WINDOW)
            .with_min_inner_size(theme::MIN_WINDOW)
            .with_title("研序 · 策略研究工作区"),
        ..Default::default()
    };
    let result = eframe::run_native(
        "研序 · 策略研究工作区",
        native,
        Box::new(|cc| {
            theme::setup_fonts(&cc.egui_ctx);
            let mut app = ResearchApp::new(false);
            // 工作区：环境变量 RESEARCH_WORKSPACE 优先，缺省 ./research-workspace
            let ws = std::env::var_os("RESEARCH_WORKSPACE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("research-workspace"));
            app.attach_bridge(ws);
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    );
    if let Err(e) = result {
        eprintln!("研究桌面启动失败：无法初始化图形后端（{e}）。");
        eprintln!("请确认运行环境具备图形显示（X11/Wayland；WSL2 需 WSLg 支持）。");
        std::process::exit(4);
    }
}
