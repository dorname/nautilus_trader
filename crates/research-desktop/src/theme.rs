//! 黑色玻璃态主题 token 与 egui 样式应用（纯数据 + 纯函数，UT-S15-08 承载）。
//!
//! 视觉规格（core-05-ai-workspace-design.md「纯黑科技 v3」）：#0a0a0a 纯黑基底，
//! 主色绿 #22C55E、辅助青 #06B6D4；侧栏与主区为悬浮圆角玻璃卡片。
//! 原型的 backdrop-filter 真实模糊为 CSS 能力，egui 即时模式无对应原语——
//! 以半透明面板色 + 顶部高光描边近似玻璃质感，设计文档视觉验收保留该口径。

use egui::{Color32, CornerRadius, Style, Vec2, Visuals};

/// 纯黑基底（窗口底色）。
pub const BASE: Color32 = Color32::from_rgb(0x0A, 0x0A, 0x0A);
/// 主色绿（导航激活、主按钮、进度）。
pub const ACCENT: Color32 = Color32::from_rgb(0x22, 0xC5, 0x5E);
/// 辅助青（次级强调、图表第二序列）。
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x06, 0xB6, 0xD4);
/// 玻璃卡片：弱（侧栏）。
pub const GLASS: Color32 = Color32::from_rgba_premultiplied(0x14, 0x16, 0x1A, 0xE6);
/// 玻璃卡片：中（主工作区）。
pub const GLASS_SOFT: Color32 = Color32::from_rgba_premultiplied(0x18, 0x1B, 0x20, 0xE0);
/// 玻璃卡片：强（浮层/对话框）。
pub const GLASS_STRONG: Color32 = Color32::from_rgba_premultiplied(0x1E, 0x22, 0x28, 0xF2);
/// 主文字。
pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xEA, 0xEF);
/// 次级文字。
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x8A, 0x93, 0x9F);
/// 分隔线/描边（顶部高光近似）。
pub const STROKE: Color32 = Color32::from_rgb(0x2A, 0x2F, 0x37);
/// 危险（失败/取消确认）。
pub const DANGER: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

/// 玻璃卡片圆角（逻辑像素）。
pub const CARD_ROUNDING: u8 = 12;

/// 文字对比度（WCAG 相对亮度比，纯函数供 UT 断言）。
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    fn lum(c: Color32) -> f64 {
        fn ch(v: u8) -> f64 {
            let v = v as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }
    let (l1, l2) = (lum(a), lum(b));
    let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

/// 应用黑色玻璃态到 egui 样式（深色 visuals、玻璃面板色、圆角、主色）。
pub fn apply(style: &mut Style) {
    let v: &mut Visuals = &mut style.visuals;
    v.dark_mode = true;
    v.override_text_color = Some(TEXT);
    v.window_fill = GLASS_STRONG;
    v.panel_fill = GLASS_SOFT;
    v.extreme_bg_color = BASE;
    v.faint_bg_color = GLASS;
    v.window_stroke = egui::Stroke::new(1.0, STROKE);
    v.window_corner_radius = CornerRadius::same(CARD_ROUNDING);
    v.menu_corner_radius = CornerRadius::same(CARD_ROUNDING);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, STROKE);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    v.widgets.inactive.bg_fill = GLASS;
    v.widgets.inactive.weak_bg_fill = GLASS;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_DIM);
    v.widgets.inactive.corner_radius = CornerRadius::same(8);
    v.widgets.hovered.weak_bg_fill = GLASS_STRONG;
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.hovered.corner_radius = CornerRadius::same(8);
    v.widgets.active.weak_bg_fill = GLASS_STRONG;
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = egui::Stroke::new(1.0, BASE);
}

/// 注册系统 CJK 字体（core-02「中文字体随包提供」：本批从系统常见路径加载，
/// 缺失时保留 egui 默认字体并在状态行提示；数字等宽由等宽族承载）。
/// 返回成功加载的字体数（0 = 未找到，调用方置字体缺失提示）。
pub fn setup_fonts(ctx: &egui::Context) -> usize {
    let candidates: &[&str] = &[
        // Linux（WSL2 常见发行版）
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
        // Windows
        "C:\\Windows\\Fonts\\msyh.ttc",
        // macOS
        "/System/Library/Fonts/PingFang.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes).into());
            // 中文回退链：默认族与等宽族都追加 CJK（egui 逐字符回退）
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push("cjk".into());
            }
            ctx.set_fonts(fonts);
            return 1;
        }
    }
    0
}

/// 默认窗口尺寸（core-02：默认 1440×900）。
pub const DEFAULT_WINDOW: Vec2 = Vec2::new(1440.0, 900.0);
/// 最小窗口尺寸（core-02：最小 1100×720）。
pub const MIN_WINDOW: Vec2 = Vec2::new(1100.0, 720.0);

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S15-08（断言之二）：主题 token 符合「纯黑科技 v3」设计值，
    /// 主/次文字对比度达到可读门槛（WCAG AA 正文 4.5:1）。
    #[test]
    fn tokens_match_design_and_text_is_readable() {
        // 设计值
        assert_eq!(BASE, Color32::from_rgb(0x0A, 0x0A, 0x0A));
        assert_eq!(ACCENT, Color32::from_rgb(0x22, 0xC5, 0x5E));
        assert_eq!(ACCENT_CYAN, Color32::from_rgb(0x06, 0xB6, 0xD4));
        // 玻璃三层互异且均为半透明
        assert_ne!(GLASS, GLASS_SOFT);
        assert_ne!(GLASS_SOFT, GLASS_STRONG);
        assert!(GLASS.a() < 255 && GLASS_SOFT.a() < 255 && GLASS_STRONG.a() < 255);
        // 对比度：主文字 ≥ 4.5，次级文字 ≥ 4.5（黑底上）
        assert!(contrast_ratio(TEXT, BASE) >= 4.5, "主文字对比度不足");
        assert!(contrast_ratio(TEXT_DIM, BASE) >= 4.5, "次级文字对比度不足");
        // 窗口尺寸
        assert_eq!(DEFAULT_WINDOW, Vec2::new(1440.0, 900.0));
        assert_eq!(MIN_WINDOW, Vec2::new(1100.0, 720.0));
    }
}
