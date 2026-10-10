//! 研究桌面原生入口：黑色玻璃态窗口（默认 1440×900，最小 880×720）。
//!
//! 无可用图形后端时以中文提示失败退出（退出码 4，与 CLI 环境错误对齐），
//! 不把软件渲染支持当作已验证承诺（core-03-desktop-delivery）。
//!
//! 启动可观测（issue #32/#1）：里程碑日志走 stderr；首帧看门狗在超时未见
//! 首帧时输出诊断并以退出码 5 退出，杜绝「进程存活但无窗口」的静默挂起。

use nautilus_research_desktop::{app::ResearchApp, theme};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 首帧看门狗超时（秒）：WSLg 冷启动较慢，给足余量。
const FIRST_FRAME_WATCHDOG_SECS: u64 = 15;

/// 首帧看门狗超时退出码（与启动失败 4 区分）。
const EXIT_FIRST_FRAME_TIMEOUT: i32 = 5;

/// 启动里程碑日志（stderr，不污染 stdout）。
fn milestone(msg: &str) {
    eprintln!("[research-desktop] {msg}");
}

/// 首帧探针包装：首个 ui() 帧到达时置位看门狗标志（issue #32：
/// CreationContext 就绪 ≠ 窗口可见/首帧渲染）。
struct FirstFrameProbe {
    app: ResearchApp,
    flag: Arc<AtomicBool>,
}

impl eframe::App for FirstFrameProbe {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if !self.flag.swap(true, Ordering::Relaxed) {
            milestone("首帧渲染到达（事件循环出帧）");
        }
        self.app.ui(ui, frame);
    }
}

fn main() {
    milestone(&format!(
        "启动 · DISPLAY={:?} WAYLAND_DISPLAY={:?} WGPU_BACKEND={:?} WINIT_UNIX_BACKEND={:?} LIBGL_ALWAYS_SOFTWARE={:?}",
        std::env::var_os("DISPLAY"),
        std::env::var_os("WAYLAND_DISPLAY"),
        std::env::var_os("WGPU_BACKEND"),
        std::env::var_os("WINIT_UNIX_BACKEND"),
        std::env::var_os("LIBGL_ALWAYS_SOFTWARE"),
    ));

    // 首帧看门狗（issue #32）：超时未见首帧 → 诊断 + 非零退出
    let first_frame = Arc::new(AtomicBool::new(false));
    {
        let flag = Arc::clone(&first_frame);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(FIRST_FRAME_WATCHDOG_SECS));
            if !flag.load(Ordering::Relaxed) {
                milestone(&format!(
                    "错误：{FIRST_FRAME_WATCHDOG_SECS}s 内未渲染首帧——进程未创建可见窗口（WSLg 已知问题）。"
                ));
                milestone(
                    "排查：1) 确认 WSLg 组件完好（wsl --shutdown 后重启）；2) 试 WINIT_UNIX_BACKEND=x11；3) 试 LIBGL_ALWAYS_SOFTWARE=1；4) 仍失败请提 issue 并附本日志。",
                );
                std::process::exit(EXIT_FIRST_FRAME_TIMEOUT);
            }
        });
    }

    let reduce_motion = nautilus_research_desktop::motion::system_reduce_motion();
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(theme::DEFAULT_WINDOW)
            .with_min_inner_size(theme::MIN_WINDOW)
            .with_title("研序 · 策略研究工作区"),
        ..Default::default()
    };
    milestone("进入 eframe::run_native（winit 事件循环 + wgpu 初始化）");
    let flag = Arc::clone(&first_frame);
    let result = eframe::run_native(
        "研序 · 策略研究工作区",
        native,
        Box::new(|cc| {
            // 到达此处：窗口与图形后端已创建（winit resumed + wgpu ready）
            milestone("窗口与图形后端已创建（CreationContext 就绪）");
            theme::setup_fonts(&cc.egui_ctx);
            milestone("字体加载完成");
            // 显式保留 Ctrl+/−/0 整窗缩放；滚轮缩放由 render_root 按修饰键门禁处理
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = true);
            let mut app = ResearchApp::new(false);
            app.reduce_motion = reduce_motion;
            // 工作区：环境变量 RESEARCH_WORKSPACE 优先，缺省 ./research-workspace
            let ws = std::env::var_os("RESEARCH_WORKSPACE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("research-workspace"));
            app.attach_bridge(ws);
            milestone("协调器桥接完成，进入事件循环");
            Ok(Box::new(FirstFrameProbe {
                app,
                flag: Arc::clone(&flag),
            }) as Box<dyn eframe::App>)
        }),
    );
    if let Err(e) = result {
        milestone(&format!("run_native 返回错误：{e}"));
        eprintln!("研究桌面启动失败：无法初始化图形后端（{e}）。");
        eprintln!("请确认运行环境具备图形显示（X11/Wayland；WSL2 需 WSLg 支持）。");
        std::process::exit(4);
    }
    milestone("事件循环正常退出");
}
