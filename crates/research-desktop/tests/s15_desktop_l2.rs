//! S15 桌面批次 L2（骨架）场景测试：导航路由 / 主题 token / 布局断点 / 会话状态。
//!
//! 规格对齐：logos/resources/test/core-S15-test-cases.md（UT-S15-07/08）。
//! 纯函数断言不触碰图形后端；渲染旅程由 ST-S15-02/03（批次 L3 后人工/双平台）承载。

use nautilus_research_desktop::layout::{self, LayoutPlan};
use nautilus_research_desktop::nav::{default_page, Page, ALL_PAGES};
use nautilus_research_desktop::session::Session;
use nautilus_research_desktop::theme;
use nautilus_research_testkit::case;

/// UT-S15-07：六页导航路由与会话状态（枚举完备、标题互异、往返与越界回退、切页滚动保留）。
#[test]
fn ut_s15_07_nav_routes_and_session() {
    case("UT-S15-07", || {
        // 六页枚举：标题非空且互异
        let titles: Vec<_> = ALL_PAGES.map(|p| p.title()).to_vec();
        for (i, t) in titles.iter().enumerate() {
            assert!(!t.is_empty(), "第 {i} 页标题为空");
            for other in titles.iter().skip(i + 1) {
                assert_ne!(t, other, "页面标题重复：{t}");
            }
        }
        // 六页集合与设计一致（流水线五页 + AI 工作台）
        assert_eq!(
            titles,
            vec!["数据快照", "股票池", "运行", "比较", "计划", "AI 工作台"]
        );
        // 顺序号往返
        for (i, p) in ALL_PAGES.iter().enumerate() {
            assert_eq!(p.index(), i);
            assert_eq!(Page::from_index(i), *p);
        }
        // 默认页与越界回退
        assert_eq!(default_page(), Page::DataSnapshot);
        assert_eq!(Page::from_index(6), Page::DataSnapshot);
        assert_eq!(Page::from_index(999), Page::DataSnapshot);

        // 会话状态：切页保留来源页滚动、目标页独立
        let mut s = Session::new();
        assert_eq!(s.page, Page::DataSnapshot);
        s.navigate(Page::AiWorkspace, 42.0);
        assert_eq!(s.page, Page::AiWorkspace);
        assert!((s.scroll_of(Page::DataSnapshot) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Page::AiWorkspace) - 0.0).abs() < 1e-6);
        s.navigate(Page::DataSnapshot, 7.0);
        assert_eq!(s.page, Page::DataSnapshot);
        assert!((s.scroll_of(Page::DataSnapshot) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Page::AiWorkspace) - 7.0).abs() < 1e-6);
    });
}

/// UT-S15-08：黑色玻璃态主题 token 与布局断点
/// （设计色值、玻璃三层半透明、文字对比度、窗口尺寸、断点切换与最小中栏可用）。
#[test]
fn ut_s15_08_theme_tokens_and_layout_breakpoints() {
    case("UT-S15-08", || {
        use egui::Color32;
        // 「纯黑科技 v3」设计值
        assert_eq!(theme::BASE, Color32::from_rgb(0x0A, 0x0A, 0x0A));
        assert_eq!(theme::ACCENT, Color32::from_rgb(0x22, 0xC5, 0x5E));
        assert_eq!(theme::ACCENT_CYAN, Color32::from_rgb(0x06, 0xB6, 0xD4));
        // 玻璃三层互异、半透明
        assert_ne!(theme::GLASS, theme::GLASS_SOFT);
        assert_ne!(theme::GLASS_SOFT, theme::GLASS_STRONG);
        assert!(
            theme::GLASS.a() < 255
                && theme::GLASS_SOFT.a() < 255
                && theme::GLASS_STRONG.a() < 255
        );
        // 对比度（WCAG AA 正文 ≥ 4.5:1）
        assert!(theme::contrast_ratio(theme::TEXT, theme::BASE) >= 4.5);
        assert!(theme::contrast_ratio(theme::TEXT_DIM, theme::BASE) >= 4.5);
        // 窗口尺寸（core-02：默认 1440×900，最小 1100×720）
        assert_eq!(theme::DEFAULT_WINDOW, egui::Vec2::new(1440.0, 900.0));
        assert_eq!(theme::MIN_WINDOW, egui::Vec2::new(1100.0, 720.0));
        // 布局断点：宽三栏 / 窄两栏
        assert_eq!(layout::plan_for_width(1440.0), LayoutPlan::ThreeCol);
        assert_eq!(layout::plan_for_width(1280.0), LayoutPlan::ThreeCol);
        assert_eq!(layout::plan_for_width(1100.0), LayoutPlan::TwoCol);
        // 最小窗口下中栏无水平溢出
        assert!(layout::min_center_usable());
        // 中栏宽度计算（右栏只在三栏扣减；负值钳 0）
        assert!(
            (layout::center_width(1440.0, LayoutPlan::ThreeCol) - 940.0).abs() < 1e-6,
            "1440 三栏中宽应为 940"
        );
        assert!(
            (layout::center_width(1100.0, LayoutPlan::TwoCol) - 920.0).abs() < 1e-6,
            "1100 两栏中宽应为 920"
        );
        assert_eq!(layout::center_width(100.0, LayoutPlan::ThreeCol), 0.0);
    });
}
