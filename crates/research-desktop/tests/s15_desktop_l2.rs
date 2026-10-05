//! S15 桌面批次 L2（骨架）场景测试：统一工作台导航路由 / 主题 token / 布局断点 / 会话状态。
//!
//! 规格对齐：logos/resources/test/core-S15-test-cases.md（UT-S15-07/08），
//! 视觉与信息架构以 core-05-ai-workspace-prototype.html 为权威。
//! 纯函数断言不触碰图形后端；渲染旅程由 ST-S15-02/03（批次 L3 后人工/双平台）承载。

use nautilus_research_desktop::layout::{self, LayoutPlan};
use nautilus_research_desktop::nav::{
    ALL_ROUTES, RESOURCE_ROUTES, Route, WORKSPACE_ROUTES, default_route,
};
use nautilus_research_desktop::session::Session;
use nautilus_research_desktop::{icons, theme};
use nautilus_research_testkit::case;

/// UT-S15-07：统一工作台导航路由与会话状态
/// （11 路由两组、标题互异、往返与越界回退、默认路由、切路由滚动保留）。
#[test]
fn ut_s15_07_nav_routes_and_session() {
    case("UT-S15-07", || {
        // 11 路由：标题非空且互异
        let titles: Vec<_> = ALL_ROUTES.map(|r| r.title()).to_vec();
        for (i, t) in titles.iter().enumerate() {
            assert!(!t.is_empty(), "第 {i} 路由标题为空");
            for other in titles.iter().skip(i + 1) {
                assert_ne!(t, other, "路由标题重复：{t}");
            }
        }
        // 11 路由集合与原型左侧栏一致（研究工作区 8 + 研究资源 3）
        assert_eq!(
            titles,
            vec![
                "项目概览",
                "需求文档",
                "策略设计",
                "策略开发",
                "事件调试",
                "回测实验",
                "验证报告",
                "交易计划",
                "数据中心",
                "股票池",
                "策略资产",
            ]
        );
        // 分组：工作区 8 路由在前、资源 3 路由在后，且 ALL = 拼接
        assert_eq!(WORKSPACE_ROUTES.len(), 8);
        assert_eq!(RESOURCE_ROUTES.len(), 3);
        assert_eq!(ALL_ROUTES.len(), 11);
        for (i, r) in WORKSPACE_ROUTES.iter().enumerate() {
            assert_eq!(ALL_ROUTES[i], *r);
            assert!(r.in_workspace_group());
        }
        for (i, r) in RESOURCE_ROUTES.iter().enumerate() {
            assert_eq!(ALL_ROUTES[8 + i], *r);
            assert!(!r.in_workspace_group());
        }
        // 顺序号往返
        for (i, r) in ALL_ROUTES.iter().enumerate() {
            assert_eq!(r.index(), i);
            assert_eq!(Route::from_index(i), *r);
        }
        // 默认路由与越界回退
        assert_eq!(default_route(), Route::Overview);
        assert_eq!(Route::from_index(11), Route::Overview);
        assert_eq!(Route::from_index(999), Route::Overview);

        // 会话状态：切路由保留来源滚动、目标路由独立（含资源组）
        let mut s = Session::new();
        assert_eq!(s.route, Route::Overview);
        s.navigate(Route::Experiments, 42.0);
        assert_eq!(s.route, Route::Experiments);
        assert!((s.scroll_of(Route::Overview) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Route::Experiments) - 0.0).abs() < 1e-6);
        s.navigate(Route::Overview, 7.0);
        assert_eq!(s.route, Route::Overview);
        assert!((s.scroll_of(Route::Overview) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Route::Experiments) - 7.0).abs() < 1e-6);
        // 资源组路由同样独立；来源滚动取当前值（模拟 GUI 每帧回写语义）
        s.navigate(Route::Data, s.scroll_of(Route::Overview));
        assert_eq!(s.route, Route::Data);
        assert!((s.scroll_of(Route::Data) - 0.0).abs() < 1e-6);
        assert!((s.scroll_of(Route::Overview) - 42.0).abs() < 1e-6);
        // 每帧回写不串扰其他路由
        s.record_scroll(Route::Data, 33.0);
        assert!((s.scroll_of(Route::Data) - 33.0).abs() < 1e-6);
        assert!((s.scroll_of(Route::Overview) - 42.0).abs() < 1e-6);
    });
}

/// UT-S15-08：纯黑科技 v3 主题 token 与统一工作台布局断点
/// （设计色值、玻璃三层半透明、文字对比度、窗口尺寸、断点切换与最小工作区可用）。
#[test]
fn ut_s15_08_theme_tokens_and_layout_breakpoints() {
    case("UT-S15-08", || {
        use egui::Color32;
        // 「纯黑科技 v3」设计值（青：原型 --cyan-text 亮档 + 设计文档基档）
        assert_eq!(theme::BASE, Color32::from_rgb(0x0A, 0x0A, 0x0A));
        assert_eq!(theme::ACCENT, Color32::from_rgb(0x22, 0xC5, 0x5E));
        assert_eq!(theme::ACCENT_CYAN, Color32::from_rgb(0x67, 0xE8, 0xF9));
        assert_eq!(theme::ACCENT_CYAN_BASE, Color32::from_rgb(0x06, 0xB6, 0xD4));
        // 玻璃三层互异、半透明
        assert_ne!(theme::GLASS, theme::GLASS_SOFT);
        assert_ne!(theme::GLASS_SOFT, theme::GLASS_STRONG);
        assert!(
            theme::GLASS.a() < 255 && theme::GLASS_SOFT.a() < 255 && theme::GLASS_STRONG.a() < 255
        );
        // 对比度（WCAG AA 正文 ≥ 4.5:1）
        assert!(theme::contrast_ratio(theme::TEXT, theme::BASE) >= 4.5);
        assert!(theme::contrast_ratio(theme::TEXT2, theme::BASE) >= 4.5);
        assert!(theme::contrast_ratio(theme::MUTED, theme::BASE) >= 4.5);
        // 窗口尺寸（core-02：默认 1440×900，最小 1100×720）
        assert_eq!(theme::DEFAULT_WINDOW, egui::Vec2::new(1440.0, 900.0));
        assert_eq!(theme::MIN_WINDOW, egui::Vec2::new(1100.0, 720.0));
        // 布局断点：>1200 舒适 / ≤1200 窄（原型 @media max-width:1200px）
        assert_eq!(layout::plan_for_width(1440.0), LayoutPlan::Comfortable);
        assert_eq!(layout::plan_for_width(1201.0), LayoutPlan::Comfortable);
        assert_eq!(layout::plan_for_width(1100.0), LayoutPlan::Compact);
        // 原型宽度常量
        assert_eq!(layout::SIDEBAR_W, 212.0);
        assert_eq!(layout::SIDEBAR_W_NARROW, 178.0);
        assert_eq!(layout::CHAT_W, 360.0);
        assert_eq!(layout::CHAT_W_NARROW, 320.0);
        assert_eq!(layout::TOPBAR_H, 65.0);
        assert_eq!(layout::FOOTER_H, 28.0);
        assert_eq!(layout::APP_GAP, 12.0);
        // 最小窗口下工作区无水平溢出
        assert!(layout::min_workspace_usable());
        // 工作区宽度计算（对话栏只在非专注模式扣减；负值钳 0）
        assert!(
            (layout::workspace_width(1440.0, LayoutPlan::Comfortable, false) - 832.0).abs() < 1e-6,
            "1440 舒适档工作区应为 832"
        );
        assert!(
            (layout::workspace_width(1100.0, LayoutPlan::Compact, false) - 566.0).abs() < 1e-6,
            "1100 窄档工作区应为 566"
        );
        assert!(
            (layout::workspace_width(1440.0, LayoutPlan::Comfortable, true) - 1192.0).abs() < 1e-6,
            "专注模式工作区应为 1192"
        );
        assert_eq!(
            layout::workspace_width(100.0, LayoutPlan::Comfortable, false),
            0.0
        );
        // 首帧守卫（desktop-firstframe-guard）：WSLg 首帧实测约 260×267，
        // 低于阈值跳帧；正常请求/最小窗口可绘制。守卫阈值下，窄窗工作区钳 0 不为负。
        assert!(
            !layout::frame_ready(260.0, 267.0),
            "WSLg 首帧尺寸应跳过绘制"
        );
        assert!(!layout::frame_ready(399.9, 900.0), "宽不足应跳过绘制");
        assert!(!layout::frame_ready(1440.0, 199.9), "高不足应跳过绘制");
        assert!(layout::frame_ready(1440.0, 900.0));
        assert!(
            layout::frame_ready(theme::MIN_WINDOW.x, theme::MIN_WINDOW.y),
            "最小窗口 1100×720 必须可绘制"
        );
        assert_eq!(
            layout::workspace_width(260.0, LayoutPlan::Compact, false),
            0.0
        );
        // 导航图标（desktop-fidelity-visuals）：11 路由全覆盖、折线非空、
        // 策略开发/策略资产同为 code（原型 routes 表）。
        for r in ALL_ROUTES {
            let kind = icons::for_route(r);
            assert!(
                icons::strokes(kind).iter().all(|s| s.len() >= 2),
                "路由 {r:?} 图标存在少于 2 点的笔画"
            );
        }
        assert_eq!(
            icons::for_route(Route::Develop),
            icons::for_route(Route::Library)
        );
        assert_eq!(icons::for_route(Route::Overview), icons::IconKind::Home);
        assert_eq!(icons::for_route(Route::Pool), icons::IconKind::Pool);
        // hero 装饰（原型 .hero-orbit）：渐变端色与双环色值、采样点规模
        assert_eq!(
            icons::HERO_GRADIENT_FROM,
            Color32::from_rgb(0x0F, 0x15, 0x12)
        );
        assert_eq!(icons::HERO_GRADIENT_TO, Color32::from_rgb(0x0A, 0x0B, 0x0A));
        let (c, e1, e2) = icons::hero_orbit_points(110.0);
        assert_eq!(c.len(), 49, "r42 圆 48 段采样");
        assert_eq!(e1.len(), 49);
        assert_eq!(e2.len(), 49);
        assert_ne!(e1, e2, "±40° 双椭圆必须互异");
        // 根布局切分（desktop-root-layout）：列/行各段非负；正常窗口 main 为正。
        let (sb, main) = layout::columns(1440.0 - 24.0, LayoutPlan::Comfortable);
        assert_eq!(sb, 212.0);
        assert!((main - (1416.0 - 212.0 - 12.0)).abs() < 1e-6);
        let (sb, main) = layout::columns(1100.0 - 24.0, LayoutPlan::Compact);
        assert_eq!(sb, 178.0);
        assert!(main > 0.0);
        // 极端窄窗：main 钳 0 不为负（侧栏宽不可压，总和允许超出——由首帧守卫兜底）
        let (_, main) = layout::columns(200.0, LayoutPlan::Comfortable);
        assert_eq!(main, 0.0);
        let (top, body, foot) = layout::rows(900.0 - 24.0);
        assert_eq!((top, foot), (65.0, 28.0));
        assert!((body - (876.0 - 65.0 - 28.0)).abs() < 1e-6);
        let (_, body, _) = layout::rows(50.0);
        assert_eq!(body, 0.0, "行高不足时 body 钳 0");
        // Slim 图标栏断点（RD-003.3）：≤900 侧栏 65、对话栏 280；>900 回到 Compact。
        assert_eq!(layout::plan_for_width(900.0), LayoutPlan::Slim);
        assert_eq!(layout::plan_for_width(901.0), LayoutPlan::Compact);
        assert_eq!(layout::sidebar_w(LayoutPlan::Slim), 65.0);
        assert_eq!(layout::chat_w(LayoutPlan::Slim), 280.0);
        assert_eq!(layout::SLIM_MAX_W, 900.0);
        assert_eq!(layout::SIDEBAR_W_SLIM, 65.0);
        assert_eq!(layout::CHAT_W_SLIM, 280.0);
        // Slim 档工作区仍有可用宽度（900 - 24 - 12 - 65 - 280 = 519）
        assert!(
            (layout::workspace_width(900.0, LayoutPlan::Slim, false) - 519.0).abs() < 1e-6,
            "900 Slim 档工作区应为 519"
        );
        // 品牌 mark 渐变端色（RD-003.1：原型 145° #4ade80 → #16a34a）
        assert_eq!(theme::ACCENT_TEXT, Color32::from_rgb(0x4A, 0xDE, 0x80));
        assert_eq!(theme::ACCENT_STRONG, Color32::from_rgb(0x16, 0xA3, 0x4A));
        // 内嵌 CJK 字体兜底（RD-003.5：随包提供承诺）
        assert!(
            theme::embedded_cjk_len() > 1_000_000,
            "内嵌字体应非空（Noto Sans SC）"
        );
        // footer 原生文案（RD-006：移除「原型/刷新重置」HTML 语义）
        assert!(
            !theme::FOOTER_LEFT.contains("原型") && !theme::FOOTER_LEFT.contains("刷新"),
            "footer 不得含 HTML 原型语义"
        );
        assert!(theme::FOOTER_LEFT.contains("本地研究桌面"));
    });
}
