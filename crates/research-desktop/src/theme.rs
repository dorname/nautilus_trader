//! 黑色玻璃态主题 token 与 egui 样式应用（纯数据 + 纯函数，UT-S15-08 承载）。
//!
//! 视觉规格（core-05-ai-workspace-design.md「纯黑科技 v3」+ 原型 CSS 变量）：
//! #0a0a0a 纯黑基底叠加绿色（左上）与青色（右下）环境柔光；主色绿 #22C55E、
//! 辅助青 #06B6D4；侧栏与主区为悬浮圆角玻璃卡片；文字四层级、语义色
//! （amber 警告 / red 错误 / accent-text 成功）；等宽数字与标签。
//! 原型的 backdrop-filter 真实模糊为 CSS 能力，egui 即时模式无对应原语——
//! 以半透明面板色 + 顶部高光描边近似玻璃质感，设计文档视觉验收保留该口径。

use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Margin, Mesh, Painter, Pos2, Stroke, Style, Vec2,
    Visuals,
};
use std::sync::atomic::{AtomicU32, Ordering};

/// 当前界面字号倍率（相对 1440×900 原型）。按钮与 `lbl` 读取此值。
static TYPE_SCALE_BITS: AtomicU32 = AtomicU32::new(0x3f800000); // 1.0

/// 写入字号倍率（窗口缩放与 Ctrl+滚轮）。
pub fn set_type_scale(scale: f32) {
    TYPE_SCALE_BITS.store(scale.clamp(0.85, 1.35).to_bits(), Ordering::Relaxed);
}

/// 当前字号倍率。
pub fn type_scale() -> f32 {
    f32::from_bits(TYPE_SCALE_BITS.load(Ordering::Relaxed))
}

/// 按当前倍率缩放原型 px。
pub fn fs(px: f32) -> f32 {
    px * type_scale()
}

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
/// 导航激活底（原型 --accent-soft：rgba(34,197,94,.08)，α≈20）。
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(2, 15, 7, 20);
/// 导航激活描边（原型 --accent-border：rgba(34,197,94,.24)，α≈61）。
pub const ACCENT_BORDER: Color32 = Color32::from_rgba_premultiplied(8, 47, 22, 61);
/// 研究路径步号描边（原型 .step-num：rgba(34,197,94,.25)，α≈64）。
pub const ACCENT_STEP_STROKE: Color32 = Color32::from_rgba_premultiplied(8, 49, 23, 64);
/// 品牌 mark 字色（原型 .brand-mark color:#03130a）。
pub const BRAND_INK: Color32 = Color32::from_rgb(0x03, 0x13, 0x0A);
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
/// 指标卡 / 行悬停浅白罩（原型 rgba(255,255,255,.03) → 预乘 (8,8,8,8)）。
/// 不可写成 (250,250,250,8)：预乘非法，egui 会画成近白底，浅色正文被吃掉。
pub const WHITE_03: Color32 = Color32::from_rgba_premultiplied(8, 8, 8, 8);
/// 对话栏底（原型 .chat rgba(20,20,22,.35)）。
pub const CHAT_BG: Color32 = Color32::from_rgba_premultiplied(7, 7, 8, 89);
/// 输入框描边（原型 .compose-box border rgba(34,197,94,.3)）。
pub const COMPOSE_STROKE: Color32 = Color32::from_rgba_premultiplied(10, 59, 28, 77);
/// 产物卡渐变起点（artifact-card 130°：rgba(34,197,94,.1)）。
pub const ARTIFACT_FROM: Color32 = Color32::from_rgba_premultiplied(3, 20, 9, 26);
/// 产物卡渐变终点（rgba(255,255,255,.02)）。
pub const ARTIFACT_TO: Color32 = Color32::from_rgba_premultiplied(5, 5, 5, 5);

/// 面板内子卡片圆角（原型 --radius 14px）。
pub const CARD_ROUNDING: u8 = 14;
/// 策略源码编辑区底色（原型 `.code`：`#0a0c0a`）。
pub const CODE_BG: Color32 = Color32::from_rgb(0x0A, 0x0C, 0x0A);
/// 策略源码文字色（原型 `.code`：`#d4e8da`）。
pub const CODE_FG: Color32 = Color32::from_rgb(0xD4, 0xE8, 0xDA);

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
    // 原型 `button{padding:8px 12px;font-size:12px;border-radius:7px}`
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    v.widgets.inactive.bg_fill = WHITE_03;
    v.widgets.inactive.weak_bg_fill = WHITE_03;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER_STRONG);
    v.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(15, 15, 15, 15);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgba_premultiplied(15, 15, 15, 15);
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 46));
}

/// 玻璃卡片容器（悬浮圆角 + 描边 + 顶部高光近似）。
pub fn glass_card(fill: Color32, radius: u8) -> Frame {
    Frame::NONE
        .fill(fill)
        .corner_radius(CornerRadius::same(radius))
        .stroke(egui::Stroke::new(1.0, BORDER))
}

/// 原型卡片外阴影 `0 8px 32px rgba(0,0,0,.4)` 的多层近似（须画在 fill 之前）。
pub fn paint_drop_shadow(painter: &Painter, rect: egui::Rect, radius: f32) {
    if !rect.is_positive() {
        return;
    }
    let layers = [(14.0_f32, 28_u8), (8.0, 36), (3.0, 48)];
    for (dy, a) in layers {
        let r = rect.translate(Vec2::new(0.0, dy * 0.35)).expand(dy * 0.15);
        painter.rect_filled(
            r,
            CornerRadius::same(radius as u8),
            Color32::from_black_alpha(a),
        );
    }
}

/// 原型 inset 顶高光 `0 1px 0 rgba(255,255,255,.05)`。
pub fn paint_inset_top(painter: &Painter, rect: egui::Rect, pad_x: f32) {
    if !rect.is_positive() {
        return;
    }
    painter.hline(
        (rect.left() + pad_x)..=(rect.right() - pad_x),
        rect.top() + 1.0,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 13)),
    );
}

/// 对话 compose 外阴影 `0 10px 32px rgba(0,0,0,.45)`。
pub fn paint_compose_shadow(painter: &Painter, rect: egui::Rect) {
    if !rect.is_positive() {
        return;
    }
    for (dy, a) in [(16.0_f32, 32_u8), (10.0, 48), (4.0, 56)] {
        let r = rect.translate(Vec2::new(0.0, dy * 0.4)).expand(2.0);
        painter.rect_filled(r, CornerRadius::same(12), Color32::from_black_alpha(a));
    }
}

/// 产物卡 130° 渐变底（原型 `.artifact-card`）。
pub fn paint_artifact_bg(painter: &Painter, rect: egui::Rect) {
    if !rect.is_positive() {
        return;
    }
    painter.add(egui::Shape::mesh(rounded_gradient_rect_alpha(
        rect,
        9.0,
        Color32::from_rgba_unmultiplied(34, 197, 94, 26),
        Color32::from_rgba_unmultiplied(255, 255, 255, 5),
        130.0,
    )));
}

/// 顶栏「演示环境」徽章：独立辉光绿点 + 文案（原型 `.tag.purple > .dot`）。
pub fn tag_live_ui(ui: &mut egui::Ui, text: &str) -> egui::Response {
    Frame::NONE
        .fill(Color32::from_rgba_premultiplied(7, 23, 13, 26))
        .stroke(egui::Stroke::new(
            1.0,
            Color32::from_rgba_premultiplied(8, 24, 14, 71),
        ))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(7, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let (dr, _) = ui.allocate_exact_size(Vec2::splat(6.0), egui::Sense::hover());
                let c = dr.center();
                ui.painter().circle_filled(
                    c,
                    5.0,
                    Color32::from_rgba_unmultiplied(34, 197, 94, 70),
                );
                ui.painter().circle_filled(c, 3.0, ACCENT);
                ui.label(
                    egui::RichText::new(text)
                        .font(FontId::monospace(fs(10.0)))
                        .color(ACCENT_TEXT),
                );
            });
        })
        .response
}

/// 原型 .brand-mark：29×29、圆角 9、145° 线性渐变 #4ade80→#16a34a、外圈辉光、字色 #03130a。
pub fn paint_brand_mark(painter: &Painter, rect: egui::Rect) {
    painter.circle_filled(
        rect.center(),
        21.0,
        Color32::from_rgba_unmultiplied(34, 197, 94, 89),
    );
    painter.add(egui::Shape::mesh(rounded_gradient_rect(
        rect,
        9.0,
        ACCENT_TEXT,
        ACCENT_STRONG,
    )));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        "研",
        FontId::proportional(fs(19.0)),
        BRAND_INK,
    );
}

/// 圆角矩形上的 CSS 145° 线性渐变（扇形三角剖分）。
fn rounded_gradient_rect(rect: egui::Rect, radius: f32, from: Color32, to: Color32) -> Mesh {
    rounded_gradient_rect_deg(rect, radius, from, to, 145.0, false)
}

/// 带角度的圆角渐变；`alpha` 为真时保留预乘/非预乘通道插值。
fn rounded_gradient_rect_alpha(
    rect: egui::Rect,
    radius: f32,
    from: Color32,
    to: Color32,
    deg: f32,
) -> Mesh {
    rounded_gradient_rect_deg(rect, radius, from, to, deg, true)
}

fn rounded_gradient_rect_deg(
    rect: egui::Rect,
    radius: f32,
    from: Color32,
    to: Color32,
    deg: f32,
    with_alpha: bool,
) -> Mesh {
    let r = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.0);
    let segs = 6_usize;
    let pi = std::f32::consts::PI;
    let half = std::f32::consts::FRAC_PI_2;
    let corners = [
        (rect.right() - r, rect.top() + r, -half, 0.0),
        (rect.right() - r, rect.bottom() - r, 0.0, half),
        (rect.left() + r, rect.bottom() - r, half, pi),
        (rect.left() + r, rect.top() + r, pi, pi + half),
    ];
    let mut pts = Vec::with_capacity(4 * (segs + 1));
    for (cx, cy, a0, a1) in corners {
        for i in 0..=segs {
            let t = i as f32 / segs as f32;
            let a = a0 + (a1 - a0) * t;
            pts.push(Pos2::new(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    let rad = deg.to_radians();
    let dir = Vec2::new(rad.sin(), -rad.cos());
    let dmin = pts
        .iter()
        .map(|q| q.to_vec2().dot(dir))
        .fold(f32::MAX, f32::min);
    let dmax = pts
        .iter()
        .map(|q| q.to_vec2().dot(dir))
        .fold(f32::MIN, f32::max);
    let span = (dmax - dmin).max(1e-6);
    let color_at = |p: Pos2| -> Color32 {
        let t = ((p.to_vec2().dot(dir) - dmin) / span).clamp(0.0, 1.0);
        if with_alpha {
            lerp_rgba(from, to, t)
        } else {
            lerp_opaque(from, to, t)
        }
    };
    let mut mesh = Mesh::default();
    let center = rect.center();
    mesh.colored_vertex(center, color_at(center));
    for p in &pts {
        mesh.colored_vertex(*p, color_at(*p));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + ((i + 1) % n));
    }
    mesh
}

fn lerp_opaque(a: Color32, b: Color32, t: f32) -> Color32 {
    let u = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * u + b.r() as f32 * t) as u8,
        (a.g() as f32 * u + b.g() as f32 * t) as u8,
        (a.b() as f32 * u + b.b() as f32 * t) as u8,
    )
}

fn lerp_rgba(a: Color32, b: Color32, t: f32) -> Color32 {
    let u = 1.0 - t;
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * u + b.r() as f32 * t) as u8,
        (a.g() as f32 * u + b.g() as f32 * t) as u8,
        (a.b() as f32 * u + b.b() as f32 * t) as u8,
        (a.a() as f32 * u + b.a() as f32 * t) as u8,
    )
}

/// 主按钮（原型 `button.primary`：绿底、字色 `#000`、字重 600、padding 8×12、圆角 7）。
pub fn primary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(12.0)))
            .color(BASE),
    )
    .fill(ACCENT)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(32.0)))
}

/// 幽灵按钮（原型 `button.ghost`：保留 3% 白底、无描边、muted 字）。
pub fn ghost_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(12.0)))
            .color(MUTED),
    )
    .fill(WHITE_03)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(32.0)))
}

/// 默认按钮（原型无 class 的 `button`：3% 白底 + `--border-strong`）。
pub fn default_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(12.0)))
            .color(TEXT2),
    )
    .fill(WHITE_03)
    .stroke(Stroke::new(1.0, BORDER_STRONG))
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(32.0)))
}

/// 小幽灵按钮（原型 `button.ghost.small`：11px、padding 5×8）。
pub fn small_ghost_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(11.0)))
            .color(MUTED),
    )
    .fill(WHITE_03)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(24.0)))
}

/// 小默认按钮（原型 `button.small`：11px、padding 5×8、强描边）。
pub fn small_default_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(11.0)))
            .color(TEXT2),
    )
    .fill(WHITE_03)
    .stroke(Stroke::new(1.0, BORDER_STRONG))
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(24.0)))
}

/// 小主按钮（原型 `button.small.primary`）。
pub fn small_primary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(11.0)))
            .color(BASE),
    )
    .fill(ACCENT)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(24.0)))
}

/// 对话建议 chip（原型 `.suggestions button`：继承全局 button 描边、10px、padding 5×8）。
pub fn chip_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text)
            .font(FontId::proportional(fs(10.0)))
            .color(MUTED),
    )
    .fill(WHITE_03)
    .stroke(Stroke::new(1.0, BORDER_STRONG))
    .corner_radius(CornerRadius::same(7))
    .wrap_mode(egui::TextWrapMode::Extend)
    .min_size(Vec2::new(0.0, fs(22.0)))
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
        TagKind::Neutral => (
            MUTED,
            Color32::from_rgba_premultiplied(13, 13, 14, 13),
            BORDER_STRONG,
        ),
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
                    .font(FontId::monospace(fs(10.0)))
                    .color(fg),
            );
        })
        .response
}

/// 分组标题（原型 .section-label：mono、letter-spacing .12em、微弱色）。
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(FontId::monospace(10.0))
            .extra_letter_spacing(1.2)
            .color(FAINT),
    );
}

/// 页眉 eyebrow（原型 `.eyebrow`：mono 9、letter-spacing .12em）。
pub fn eyebrow_label(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .font(FontId::monospace(fs(9.0)))
            .extra_letter_spacing(1.08)
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

/// 内嵌 CJK 字体（core-02「中文字体随包提供」兜底）：Noto Sans SC Regular，
/// SIL Open Font License 可再分发；系统探测全部失败时兜底，消除字体缺失告警。
const EMBEDDED_CJK: &[u8] = include_bytes!("../assets/NotoSansSC-Regular.otf");

/// footer 左侧声明（RD-006：原生桌面语义，移除「原型/刷新重置」）。
pub const FOOTER_LEFT: &str = "● 本地研究桌面 · 合成/导入样本可追溯 · 结果绑定版本与快照";

/// 内嵌字体字节数（UT-S15-08 断言非空）。
pub fn embedded_cjk_len() -> usize {
    EMBEDDED_CJK.len()
}

/// 注册 CJK 字体（优先系统常见路径，失败回退内嵌字体）。
/// 返回成功加载的字体数（≥1 = 中文可用；0 = 连内嵌都失败，仅理论上可能）。
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
            install_cjk(ctx, egui::FontData::from_owned(bytes));
            return 1;
        }
    }
    // 系统路径全部缺失：内嵌字体兜底（随包提供承诺）
    install_cjk(ctx, egui::FontData::from_static(EMBEDDED_CJK));
    1
}

/// 把一份 CJK 字体数据注册为默认族首位（中文 UI 用同一套度量，避免
/// `.strong()` / Ubuntu 回退导致按钮内文字垂直错位）。
fn install_cjk(ctx: &egui::Context, mut data: egui::FontData) {
    data.tweak.y_offset_factor = 0.06;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("cjk".into(), data.into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = fonts.families.entry(family).or_default();
        list.retain(|n| n != "cjk");
        list.insert(0, "cjk".into());
    }
    ctx.set_fonts(fonts);
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
        assert_eq!(BRAND_INK, Color32::from_rgb(0x03, 0x13, 0x0A));
        assert_eq!(ACCENT_SOFT, Color32::from_rgba_premultiplied(2, 15, 7, 20));
        assert_eq!(
            ACCENT_BORDER,
            Color32::from_rgba_premultiplied(8, 47, 22, 61)
        );
        assert_eq!(AMBER, Color32::from_rgb(0xFB, 0xBF, 0x24));
        assert_eq!(RED, Color32::from_rgb(0xF8, 0x71, 0x71));
        // 文字四层级（--text/--text2/--muted/--faint）
        assert_eq!(TEXT, Color32::from_rgb(0xFA, 0xFA, 0xFA));
        assert_eq!(TEXT2, Color32::from_rgb(0xD4, 0xD4, 0xD8));
        assert_eq!(MUTED, Color32::from_rgb(0xA1, 0xA1, 0xAA));
        assert_eq!(FAINT, Color32::from_rgb(0x71, 0x71, 0x7A));
        assert_eq!(TEXT_DIM, MUTED);
        let mut style = Style::default();
        apply(&mut style);
        assert_eq!(style.spacing.button_padding, Vec2::new(12.0, 8.0));
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
