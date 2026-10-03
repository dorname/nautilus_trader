//! 黑色玻璃态主题 token 与 egui 样式应用（纯数据 + 纯函数，UT-S15-08 承载）。
//!
//! 视觉规格（core-05-ai-workspace-design.md「纯黑科技 v3」+ 原型 CSS 变量）：
//! #0a0a0a 纯黑基底叠加绿色（左上）与青色（右下）环境柔光；主色绿 #22C55E、
//! 辅助青 #06B6D4；侧栏与主区为悬浮圆角玻璃卡片；文字四层级、语义色
//! （amber 警告 / red 错误 / accent-text 成功）；等宽数字与标签。
//! 原型的 backdrop-filter 真实模糊为 CSS 能力，egui 即时模式无对应原语——
//! 以半透明面板色 + 顶部高光描边近似玻璃质感，设计文档视觉验收保留该口径。

use egui::{Color32, CornerRadius, FontId, Frame, Margin, Mesh, Pos2, Style, Vec2, Visuals};

/// 纯黑基底（窗口底色，--bg）。
pub const BASE: Color32 = Color32::from_rgb(0x0A, 0x0A, 0x0A);
/// 主色绿（导航激活、主按钮、进度，--accent）。
pub const ACCENT: Color32 = Color32::from_rgb(0x22, 0xC5, 0x5E);
/// 主色绿·深（主按钮悬停，--accent-strong）。
pub const ACCENT_STRONG: Color32 = Color32::from_rgb(0x16, 0xA3, 0x4A);
/// 成功/强调文字绿（--accent-text）。
pub const ACCENT_TEXT: Color32 = Color32::from_rgb(0x4A, 0xDE, 0x80);
/// 辅助青·亮档（原型 --cyan-text，用于文字与次级强调）。
pub const ACCENT_CYAN: Color32 = Color32::from_rgb(0x67, 0xE8, 0xF9);
/// 辅助青·基档（设计文档「纯黑科技 v3」辅助青；环境柔光与图表第二序列）。
pub const ACCENT_CYAN_BASE: Color32 = Color32::from_rgb(0x06, 0xB6, 0xD4);
/// 警告琥珀（--amber-text）。
pub const AMBER: Color32 = Color32::from_rgb(0xFB, 0xBF, 0x24);
/// 错误红（--red-text）。
pub const RED: Color32 = Color32::from_rgb(0xF8, 0x71, 0x71);

/// 主文字（--text）。
pub const TEXT: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xFA);
/// 正文次级文字（--text2）。
pub const TEXT2: Color32 = Color32::from_rgb(0xD4, 0xD4, 0xD8);
/// 弱化文字（--muted）。
pub const MUTED: Color32 = Color32::from_rgb(0xA1, 0xA1, 0xAA);
/// 装饰性微弱文字（--faint；仅用于分组标题等非正文场景）。
pub const FAINT: Color32 = Color32::from_rgb(0x71, 0x71, 0x7A);
/// 次级文字别名（历史 UT 引用；= MUTED）。
pub const TEXT_DIM: Color32 = MUTED;

/// 描边（--border，白 8% 近似）。
pub const BORDER: Color32 = Color32::from_rgb(0x24, 0x25, 0x28);
/// 强描边（--border-strong，白 14% 近似）。
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x33, 0x35, 0x39);
/// 分隔线/旧描边别名。
pub const STROKE: Color32 = BORDER;

/// 玻璃卡片：弱（侧栏，--glass）。
pub const GLASS: Color32 = Color32::from_rgba_premultiplied(0x14, 0x16, 0x1A, 0xE6);
/// 玻璃卡片：中（主工作区，--glass-soft）。
pub const GLASS_SOFT: Color32 = Color32::from_rgba_premultiplied(0x18, 0x1B, 0x20, 0xE0);
/// 玻璃卡片：强（浮层/对话框，--glass-strong）。
pub const GLASS_STRONG: Color32 = Color32::from_rgba_premultiplied(0x1E, 0x22, 0x28, 0xF2);
/// 输入底（rgba(0,0,0,.35) 近似）。
pub const INPUT_BG: Color32 = Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x59);
/// 主卡片底（原型 .main rgba(15,15,17,.5)）。
pub const MAIN_BG: Color32 = Color32::from_rgba_premultiplied(0x08, 0x08, 0x09, 0x80);

/// 面板内子卡片圆角（原型 --radius 14px）。
pub const CARD_ROUNDING: u8 = 14;

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
    v.extreme_bg_color = INPUT_BG;
    v.faint_bg_color = GLASS;
    v.window_stroke = egui::Stroke::new(1.0, BORDER);
    v.window_corner_radius = CornerRadius::same(CARD_ROUNDING);
    v.menu_corner_radius = CornerRadius::same(CARD_ROUNDING);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.corner_radius = CornerRadius::same(7);
    v.widgets.inactive.bg_fill = GLASS;
    v.widgets.inactive.weak_bg_fill = GLASS;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT2);
    v.widgets.inactive.corner_radius = CornerRadius::same(7);
    v.widgets.hovered.weak_bg_fill = GLASS_STRONG;
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.hovered.corner_radius = CornerRadius::same(7);
    v.widgets.active.weak_bg_fill = GLASS_STRONG;
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.corner_radius = CornerRadius::same(7);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = egui::Stroke::new(1.0, BASE);
}

/// 玻璃卡片容器（悬浮圆角 + 描边 + 顶部高光近似）。
pub fn glass_card(fill: Color32, radius: u8) -> Frame {
    Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(radius))
        .stroke(egui::Stroke::new(1.0, BORDER))
}

/// 主按钮（原型 button.primary：绿底黑字）。
pub fn primary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(12.0))
            .color(BASE),
    )
    .fill(ACCENT)
    .corner_radius(CornerRadius::same(7))
}

/// 幽灵按钮（原型 button.ghost：弱文字、弱描边）。
pub fn ghost_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(12.0))
            .color(MUTED),
    )
    .fill(Color32::TRANSPARENT)
    .stroke(egui::Stroke::new(1.0, BORDER_STRONG))
    .corner_radius(CornerRadius::same(7))
}

/// 徽章类型（原型 .tag 系列色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagKind {
    /// 中性（描边灰）。
    Neutral,
    /// 强调绿（--tag purple 变体）。
    Accent,
    /// 警告琥珀。
    Warn,
    /// 错误红。
    Bad,
}

/// 徽章（原型 .tag：mono 10px、圆角 5、着色描边；返回可点击响应）。
pub fn tag_ui(ui: &mut egui::Ui, text: &str, kind: TagKind) -> egui::Response {
    let (fg, bg, bd) = match kind {
        TagKind::Neutral => (MUTED, Color32::from_rgba_premultiplied(13, 13, 14, 13), BORDER_STRONG),
        TagKind::Accent => (
            ACCENT_TEXT,
            Color32::from_rgba_premultiplied(7, 23, 13, 26),
            Color32::from_rgba_premultiplied(8, 24, 14, 71),
        ),
        TagKind::Warn => (
            AMBER,
            Color32::from_rgba_premultiplied(39, 28, 5, 15),
            Color32::from_rgba_premultiplied(39, 28, 5, 71),
        ),
        TagKind::Bad => (
            RED,
            Color32::from_rgba_premultiplied(38, 12, 12, 26),
            Color32::from_rgba_premultiplied(38, 12, 12, 71),
        ),
    };
    Frame::NONE
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, bd))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(7, 4))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .font(FontId::monospace(10.0))
                    .color(fg),
            );
        })
        .response
}

/// 分组标题（原型 .section-label：mono 小号、微弱色）。
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(FontId::monospace(10.0))
            .color(FAINT),
    );
}

/// 环境柔光（原型 body 双径向渐变：左上绿 rgba(34,197,94,.13)、
/// 右下青 rgba(6,182,212,.11)——顶点色网格线性插值近似，视觉验收口径同玻璃模糊）。
pub fn paint_ambient(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    if !rect.is_positive() {
        return;
    }
    // 预乘近似：green(34,197,94)×.13 → (4,25,12,33)；cyan(6,182,212)×.11 → (1,20,23,28)
    glow(
        ui,
        rect,
        rect.left_top(),
        Color32::from_rgba_premultiplied(0x04, 0x19, 0x0C, 0x21),
    );
    glow(
        ui,
        rect,
        rect.right_bottom(),
        Color32::from_rgba_premultiplied(0x01, 0x14, 0x17, 0x1C),
    );
}

/// 单个角部柔光网格（角点着色、其余透明，线性插值成柔和渐变）。
fn glow(ui: &mut egui::Ui, rect: egui::Rect, corner: Pos2, color: Color32) {
    let clear = Color32::TRANSPARENT;
    let (tl, tr, br, bl) = (
        Pos2::new(rect.left(), rect.top()),
        Pos2::new(rect.right(), rect.top()),
        Pos2::new(rect.right(), rect.bottom()),
        Pos2::new(rect.left(), rect.bottom()),
    );
    let near = |p: Pos2| if p == corner { color } else { clear };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(tl, near(tl));
    mesh.colored_vertex(tr, near(tr));
    mesh.colored_vertex(br, near(br));
    mesh.colored_vertex(bl, near(bl));
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
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

    /// UT-S15-08（断言之组一）：主题 token 符合「纯黑科技 v3」设计值，
    /// 主/次文字对比度达到可读门槛（WCAG AA 正文 4.5:1）。
    #[test]
    fn tokens_match_design_and_text_is_readable() {
        // 设计值：基底与主辅色
        assert_eq!(BASE, Color32::from_rgb(0x0A, 0x0A, 0x0A));
        assert_eq!(ACCENT, Color32::from_rgb(0x22, 0xC5, 0x5E));
        assert_eq!(ACCENT_STRONG, Color32::from_rgb(0x16, 0xA3, 0x4A));
        assert_eq!(ACCENT_TEXT, Color32::from_rgb(0x4A, 0xDE, 0x80));
        // 语义色（原型 --cyan-text 亮档 + 设计文档辅助青基档/--amber-text/--red-text）
        assert_eq!(ACCENT_CYAN, Color32::from_rgb(0x67, 0xE8, 0xF9));
        assert_eq!(ACCENT_CYAN_BASE, Color32::from_rgb(0x06, 0xB6, 0xD4));
        assert_eq!(AMBER, Color32::from_rgb(0xFB, 0xBF, 0x24));
        assert_eq!(RED, Color32::from_rgb(0xF8, 0x71, 0x71));
        // 文字四层级（--text/--text2/--muted/--faint）
        assert_eq!(TEXT, Color32::from_rgb(0xFA, 0xFA, 0xFA));
        assert_eq!(TEXT2, Color32::from_rgb(0xD4, 0xD4, 0xD8));
        assert_eq!(MUTED, Color32::from_rgb(0xA1, 0xA1, 0xAA));
        assert_eq!(FAINT, Color32::from_rgb(0x71, 0x71, 0x7A));
        assert_eq!(TEXT_DIM, MUTED);
        // 玻璃三层互异且均为半透明
        assert_ne!(GLASS, GLASS_SOFT);
        assert_ne!(GLASS_SOFT, GLASS_STRONG);
        assert!(GLASS.a() < 255 && GLASS_SOFT.a() < 255 && GLASS_STRONG.a() < 255);
        // 对比度：主文字与正文次级文字 ≥ 4.5（黑底上）
        assert!(contrast_ratio(TEXT, BASE) >= 4.5, "主文字对比度不足");
        assert!(contrast_ratio(TEXT2, BASE) >= 4.5, "次级文字对比度不足");
        assert!(contrast_ratio(TEXT_DIM, BASE) >= 4.5, "弱化文字对比度不足");
        assert!(contrast_ratio(ACCENT_TEXT, BASE) >= 4.5, "强调绿对比度不足");
        // 窗口尺寸
        assert_eq!(DEFAULT_WINDOW, Vec2::new(1440.0, 900.0));
        assert_eq!(MIN_WINDOW, Vec2::new(1100.0, 720.0));
    }
}
