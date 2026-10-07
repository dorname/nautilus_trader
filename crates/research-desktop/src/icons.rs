//! 左侧栏导航图标（UT-S15-08 承载）——权威原型 10 组 SVG path 的 egui 转写。
//!
//! 原型 core-05-ai-workspace-prototype.html：`viewBox 0 0 24 24`、
//! stroke-width 1.6、fill none、stroke=currentColor。SVG path 中的曲线在此
//! 按直线/圆弧近似展开为折线点列（painter.path_stroke 分笔画描边），
//! 视觉误差 <0.5px（16px 显示尺寸下不可辨）。

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

use crate::nav::Route;

/// 图标种类（原型 `icons` 对象的键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconKind {
    /// 项目概览（房子）。
    Home,
    /// 需求文档。
    File,
    /// 策略设计（流程节点）。
    Flow,
    /// 策略开发 / 策略资产（尖括号代码）。
    Code,
    /// 事件调试（甲虫）。
    Debug,
    /// 回测实验（折线坐标）。
    Chart,
    /// 验证报告（盾与勾）。
    Check,
    /// 交易计划（清单）。
    Plan,
    /// 数据中心（数据库）。
    Data,
    /// 股票池（四宫格）。
    Pool,
    /// 箭头（产物卡右侧）。
    Arrow,
}

/// 原型 stroke-width 1.6 在 16px 显示尺寸下的等比线宽（16/24 × 1.6 ≈ 1.07，取 1.2 增强可读性）。
const STROKE_W: f32 = 1.2;

/// 24×24 viewBox 内的一笔折线（SVG path 中一个子路径，坐标已按原型）。
type Stroke24 = &'static [(f32, f32)];

/// 原型 path：`M3 10 12 3l9 7v10H3Z M9 20v-7h6v7`
const HOME: &[Stroke24] = &[
    &[
        (3.0, 10.0),
        (12.0, 3.0),
        (21.0, 10.0),
        (21.0, 20.0),
        (3.0, 20.0),
        (3.0, 10.0),
    ],
    &[(9.0, 20.0), (9.0, 13.0), (15.0, 13.0), (15.0, 20.0)],
];
/// 原型 path：`M6 3h8l4 4v14H6Z M14 3v5h4 M9 12h6 M9 16h6`
const FILE: &[Stroke24] = &[
    &[
        (6.0, 3.0),
        (14.0, 3.0),
        (18.0, 7.0),
        (18.0, 21.0),
        (6.0, 21.0),
        (6.0, 3.0),
    ],
    &[(14.0, 3.0), (14.0, 8.0), (18.0, 8.0)],
    &[(9.0, 12.0), (15.0, 12.0)],
    &[(9.0, 16.0), (15.0, 16.0)],
];
/// 原型 path：`M8 3h8v5H8Z M3 16h7v5H3Z M14 16h7v5h-7Z M12 8v4M6 16v-4h12v4`
const FLOW: &[Stroke24] = &[
    &[(8.0, 3.0), (16.0, 3.0), (16.0, 8.0), (8.0, 8.0), (8.0, 3.0)],
    &[
        (3.0, 16.0),
        (10.0, 16.0),
        (10.0, 21.0),
        (3.0, 21.0),
        (3.0, 16.0),
    ],
    &[
        (14.0, 16.0),
        (21.0, 16.0),
        (21.0, 21.0),
        (14.0, 21.0),
        (14.0, 16.0),
    ],
    &[(12.0, 8.0), (12.0, 12.0)],
    &[(6.0, 16.0), (6.0, 12.0), (18.0, 12.0), (18.0, 16.0)],
];
/// 原型 path：`m8 7-5 5 5 5m8-10 5 5-5 5M14 4l-4 16`
const CODE: &[Stroke24] = &[
    &[(8.0, 7.0), (3.0, 12.0), (8.0, 17.0)],
    &[(16.0, 7.0), (21.0, 12.0), (16.0, 17.0)],
    &[(14.0, 4.0), (10.0, 20.0)],
];
/// 原型 path（甲虫）：`M8 8h8v9a4 4 0 0 1-8 0Z M9 8V6a3 3 0 0 1 6 0v2
/// M3 10h5m8 0h5M3 15h5m8 0h5M5 21l4-3m6 0 4 3`——圆弧以 5 段折线近似。
const DEBUG: &[Stroke24] = &[
    // 躯体：M8 8 → h8 → v9 → a4 4 圆弧回到底边中点 → Z
    &[
        (8.0, 8.0),
        (16.0, 8.0),
        (16.0, 17.0),
        (14.8, 19.4),
        (12.0, 21.0),
        (9.2, 19.4),
        (8.0, 17.0),
        (8.0, 8.0),
    ],
    // 头部圆弧：M9 8 V6 a3 3 0 0 1 6 0 v2
    &[
        (9.0, 8.0),
        (9.0, 6.0),
        (10.0, 4.1),
        (12.0, 3.0),
        (14.0, 4.1),
        (15.0, 6.0),
        (15.0, 8.0),
    ],
    &[(3.0, 10.0), (8.0, 10.0)],
    &[(16.0, 10.0), (21.0, 10.0)],
    &[(3.0, 15.0), (8.0, 15.0)],
    &[(16.0, 15.0), (21.0, 15.0)],
    &[(5.0, 21.0), (9.0, 18.0)],
    &[(15.0, 18.0), (19.0, 21.0)],
];
/// 原型 path：`M4 3v17h17 M7 15l4-5 4 3 5-8`
const CHART: &[Stroke24] = &[
    &[(4.0, 3.0), (4.0, 20.0), (21.0, 20.0)],
    &[(7.0, 15.0), (11.0, 10.0), (15.0, 13.0), (20.0, 5.0)],
];
/// 原型 path（盾与勾）：`M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6Z m-4 9 3 3 5-6`
const CHECK: &[Stroke24] = &[
    &[
        (12.0, 3.0),
        (4.0, 6.0),
        (4.0, 12.0),
        (6.5, 16.0),
        (12.0, 21.0),
        (17.5, 16.0),
        (20.0, 12.0),
        (20.0, 6.0),
        (12.0, 3.0),
    ],
    &[(8.0, 12.0), (11.0, 15.0), (16.0, 9.0)],
];
/// 原型 path：`M5 3h14v18H5Z M8 8h8M8 12h8M8 16h5`
const PLAN: &[Stroke24] = &[
    &[
        (5.0, 3.0),
        (19.0, 3.0),
        (19.0, 21.0),
        (5.0, 21.0),
        (5.0, 3.0),
    ],
    &[(8.0, 8.0), (16.0, 8.0)],
    &[(8.0, 12.0), (16.0, 12.0)],
    &[(8.0, 16.0), (13.0, 16.0)],
];
/// 原型 path（数据库）：`M4 6c0-4 16-4 16 0s-16 4-16 0v12c0 4 16 4 16 0V6M4 12c0 4 16 4 16 0`
/// 椭圆按 8 段折线近似（rx=8 ry=2）。
const DATA: &[Stroke24] = &[
    // 顶面椭圆（从左侧顺时针）
    &[
        (4.0, 6.0),
        (6.3, 4.6),
        (10.0, 4.0),
        (14.0, 4.0),
        (17.7, 4.6),
        (20.0, 6.0),
        (17.7, 7.4),
        (14.0, 8.0),
        (10.0, 8.0),
        (6.3, 7.4),
        (4.0, 6.0),
    ],
    // 左壁
    &[(4.0, 6.0), (4.0, 18.0)],
    // 右壁
    &[(20.0, 6.0), (20.0, 18.0)],
    // 底面下沿弧线
    &[
        (4.0, 18.0),
        (6.3, 19.4),
        (10.0, 20.0),
        (14.0, 20.0),
        (17.7, 19.4),
        (20.0, 18.0),
    ],
    // 中部截面弧线
    &[
        (4.0, 12.0),
        (6.3, 13.4),
        (10.0, 14.0),
        (14.0, 14.0),
        (17.7, 13.4),
        (20.0, 12.0),
    ],
];
/// 原型 path：`M3 3h7v7H3Z M14 3h7v7h-7Z M3 14h7v7H3Z M14 14h7v7h-7Z`
const POOL: &[Stroke24] = &[
    &[
        (3.0, 3.0),
        (10.0, 3.0),
        (10.0, 10.0),
        (3.0, 10.0),
        (3.0, 3.0),
    ],
    &[
        (14.0, 3.0),
        (21.0, 3.0),
        (21.0, 10.0),
        (14.0, 10.0),
        (14.0, 3.0),
    ],
    &[
        (3.0, 14.0),
        (10.0, 14.0),
        (10.0, 21.0),
        (3.0, 21.0),
        (3.0, 14.0),
    ],
    &[
        (14.0, 14.0),
        (21.0, 14.0),
        (21.0, 21.0),
        (14.0, 21.0),
        (14.0, 14.0),
    ],
];
/// 原型 path：`M4 12h16m-6-6 6 6-6 6`
const ARROW: &[Stroke24] = &[
    &[(4.0, 12.0), (20.0, 12.0)],
    &[(14.0, 6.0), (20.0, 12.0), (14.0, 18.0)],
];

/// 图标种类的折线笔画集合。
pub fn strokes(kind: IconKind) -> &'static [Stroke24] {
    match kind {
        IconKind::Home => HOME,
        IconKind::File => FILE,
        IconKind::Flow => FLOW,
        IconKind::Code => CODE,
        IconKind::Debug => DEBUG,
        IconKind::Chart => CHART,
        IconKind::Check => CHECK,
        IconKind::Plan => PLAN,
        IconKind::Data => DATA,
        IconKind::Pool => POOL,
        IconKind::Arrow => ARROW,
    }
}

/// 路由 → 图标分发（原型 `routes` 表第二列；策略开发/策略资产同为 code）。
pub fn for_route(route: Route) -> IconKind {
    match route {
        Route::Overview => IconKind::Home,
        Route::Requirements => IconKind::File,
        Route::Design => IconKind::Flow,
        Route::Develop => IconKind::Code,
        Route::Debug => IconKind::Debug,
        Route::Experiments => IconKind::Chart,
        Route::Validate => IconKind::Check,
        Route::Plan => IconKind::Plan,
        Route::Data => IconKind::Data,
        Route::Pool => IconKind::Pool,
        Route::Library => IconKind::Code,
    }
}

/// 在给定矩形内描边绘制图标（viewBox 24×24 等比缩放、居中，stroke=color）。
pub fn paint_icon(painter: &Painter, rect: Rect, kind: IconKind, color: Color32) {
    let side = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(side / 2.0);
    let scale = side / 24.0;
    let stroke = Stroke::new(STROKE_W * scale, color);
    for stroke24 in strokes(kind) {
        let points: Vec<Pos2> = stroke24
            .iter()
            .map(|&(x, y)| origin + Vec2::new(x * scale, y * scale))
            .collect();
        if points.len() >= 2 {
            painter.add(egui::Shape::line(points, stroke));
        }
    }
}

// ---------------------------------------------------------------- hero 装饰

/// 原型 .hero：linear-gradient(110deg, #0f1512, #0a0b0a) + radial 绿晕 88% 顶。
/// 渐变端色（纯函数，UT-S15-08 承载）。
pub const HERO_GRADIENT_FROM: Color32 = Color32::from_rgb(0x0F, 0x15, 0x12);
/// 渐变端色（110° 方向终点）。
pub const HERO_GRADIENT_TO: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x0A);
/// 原型 hero-orbit 绿（#22c55e，opacity .5；窄窗 .3）。
pub const HERO_ORBIT: Color32 = Color32::from_rgba_premultiplied(0x22, 0xC5, 0x5E, 0x80);
/// 窄窗 hero-orbit 透明度档（原型 @1200 媒体查询 opacity:.3）。
pub const HERO_ORBIT_NARROW: Color32 = Color32::from_rgba_premultiplied(0x22, 0xC5, 0x5E, 0x4D);
/// 原型 hero-orbit 中心点（#4ade80，opacity .5）。
pub const HERO_ORBIT_CORE: Color32 = Color32::from_rgba_premultiplied(0x4A, 0xDE, 0x80, 0x80);

/// 原型 hero-orbit：r42 圆 + 双椭圆（rx23 ry50，±40° 旋转）+ r6 实心点，
/// viewBox 120 缩放至 size。返回圆点采样（stroke 折线）与中心点（纯函数可测）。
pub fn hero_orbit_points(size: f32) -> (Vec<(f32, f32)>, Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let scale = size / 120.0;
    let cx = 60.0 * scale;
    let cy = 60.0 * scale;
    let circle: Vec<(f32, f32)> = (0..=48)
        .map(|i| {
            let a = i as f32 / 48.0 * std::f32::consts::TAU;
            (cx + 42.0 * scale * a.cos(), cy + 42.0 * scale * a.sin())
        })
        .collect();
    let ellipse = |deg: f32| -> Vec<(f32, f32)> {
        let rad = deg.to_radians();
        let (c, s) = (rad.cos(), rad.sin());
        (0..=48)
            .map(|i| {
                let a = i as f32 / 48.0 * std::f32::consts::TAU;
                let (ex, ey) = (23.0 * scale * a.cos(), 50.0 * scale * a.sin());
                (cx + ex * c - ey * s, cy + ex * s + ey * c)
            })
            .collect()
    };
    (circle, ellipse(40.0), ellipse(-40.0))
}

/// 绘制 hero-orbit（居中于 rect）。
pub fn paint_hero_orbit(painter: &Painter, rect: Rect, narrow: bool) {
    let size = rect.width().min(rect.height());
    let origin = rect.center() - Vec2::splat(size / 2.0);
    let color = if narrow {
        HERO_ORBIT_NARROW
    } else {
        HERO_ORBIT
    };
    let (circle, e1, e2) = hero_orbit_points(size);
    let stroke = Stroke::new(1.0, color);
    for pts in [&circle, &e1, &e2] {
        let points: Vec<Pos2> = pts.iter().map(|&(x, y)| origin + Vec2::new(x, y)).collect();
        painter.add(egui::Shape::line(points, stroke));
    }
    painter.circle_filled(rect.center(), 6.0 * size / 120.0, HERO_ORBIT_CORE);
}

/// 助手徽标四角星（替代内嵌字体缺失的 ✧）。
pub fn paint_sparkle(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.42;
    let stroke = Stroke::new(1.2, color);
    painter.line_segment([c - Vec2::new(0.0, r), c + Vec2::new(0.0, r)], stroke);
    painter.line_segment([c - Vec2::new(r, 0.0), c + Vec2::new(r, 0.0)], stroke);
    let d = r * 0.62;
    painter.line_segment([c - Vec2::new(d, d), c + Vec2::new(d, d)], stroke);
    painter.line_segment([c - Vec2::new(d, -d), c + Vec2::new(d, -d)], stroke);
}

/// 工作区头菱形（替代内嵌字体缺失的 ◇）。
pub fn paint_diamond(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.42;
    let pts = [
        c + Vec2::new(0.0, -r),
        c + Vec2::new(r, 0.0),
        c + Vec2::new(0.0, r),
        c + Vec2::new(-r, 0.0),
        c + Vec2::new(0.0, -r),
    ];
    painter.add(egui::Shape::line(pts.to_vec(), Stroke::new(1.2, color)));
}

/// 展开工作区图标（替代内嵌字体缺失的 ⤢：对角双向箭头）。
pub fn paint_expand(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.38;
    let stroke = Stroke::new(1.35, color);
    // 主对角线
    painter.line_segment([c + Vec2::new(-r, -r), c + Vec2::new(r, r)], stroke);
    // 左上箭头
    let nw = c + Vec2::new(-r, -r);
    painter.line_segment([nw, nw + Vec2::new(r * 0.55, 0.0)], stroke);
    painter.line_segment([nw, nw + Vec2::new(0.0, r * 0.55)], stroke);
    // 右下箭头
    let se = c + Vec2::new(r, r);
    painter.line_segment([se, se + Vec2::new(-r * 0.55, 0.0)], stroke);
    painter.line_segment([se, se + Vec2::new(0.0, -r * 0.55)], stroke);
}

/// 任务条旋转环（原型 `.task-spin`：10px 圆环、accent-text 顶弧）。
pub fn paint_task_spin(painter: &Painter, rect: Rect, phase: f32, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.42;
    let dim = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70);
    painter.circle_stroke(c, r, Stroke::new(1.5, dim));
    let segs = 10;
    let span = std::f32::consts::TAU * 0.28;
    let mut pts = Vec::with_capacity(segs + 1);
    for i in 0..=segs {
        let a = phase + span * (i as f32 / segs as f32) - std::f32::consts::FRAC_PI_2;
        pts.push(c + Vec2::new(r * a.cos(), r * a.sin()));
    }
    painter.add(egui::Shape::line(pts, Stroke::new(1.6, color)));
}

/// 发送箭头（对话 compose 圆形主按钮内）。
pub fn paint_send_up(painter: &Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) * 0.28;
    let stroke = Stroke::new(1.6, color);
    let tip = c + Vec2::new(0.0, -r);
    painter.line_segment([c + Vec2::new(0.0, r), tip], stroke);
    painter.line_segment([tip, tip + Vec2::new(-r * 0.7, r * 0.7)], stroke);
    painter.line_segment([tip, tip + Vec2::new(r * 0.7, r * 0.7)], stroke);
}
