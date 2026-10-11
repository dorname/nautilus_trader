//! RD-003 关闭证据：真实 egui 绘制、导航、字体与动态效果。
#[path = "support/profile.rs"]
mod test_profile;

use egui_kittest::{Harness, kittest::Queryable};
use nautilus_research_desktop::{app::ResearchApp, icons, layout, motion, nav::ALL_ROUTES, theme};
use nautilus_research_testkit::case;

fn harness(width: f32) -> Harness<'static, ResearchApp> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(
            width,
            if width == 880.0 { 720.0 } else { 900.0 },
        ))
        .build_ui_state(
            |ui, app: &mut ResearchApp| {
                // kittest 的 UI 包装默认加 8px 外边距；原生入口直接使用完整根矩形。
                let mut root = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(ui.ctx().content_rect())
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                root.set_clip_rect(ui.ctx().content_rect());
                app.render_root(&mut root);
            },
            ResearchApp::new(false),
        );
    theme::setup_embedded_fonts(&h.ctx);
    h.run_steps(3);
    h
}

fn frame(width: f32) -> Vec<egui::epaint::ClippedShape> {
    let ctx = egui::Context::default();
    theme::setup_embedded_fonts(&ctx);
    let mut app = ResearchApp::new(false);
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, if width == 880.0 { 720.0 } else { 900.0 }),
                )),
                ..Default::default()
            },
            |ui| app.render_root(ui),
        );
        shapes = std::mem::take(&mut out.shapes);
        out.drop_without_applying_deltas();
    }
    shapes
}

#[test]
fn ut_s15_12_brand_and_embedded_font() {
    case("UT-S15-12", || {
        let ctx = egui::Context::default();
        theme::setup_embedded_fonts(&ctx);
        let rect = egui::Rect::from_min_size(egui::pos2(30.0, 30.0), egui::vec2(29.0, 29.0));
        let out = ctx.run_ui(Default::default(), |ui| {
            ui.fonts_mut(|f| {
                assert!(
                    f.has_glyphs(&egui::FontId::proportional(19.0), "研序策略研究工作区"),
                    "内嵌字体必须包含中文实际字形"
                )
            });
            theme::paint_brand_mark(ui.painter(), rect);
        });
        let mesh = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Mesh(m) => Some(m),
                _ => None,
            })
            .expect("品牌渐变网格");
        let colors: std::collections::HashSet<_> = mesh.vertices.iter().map(|v| v.color).collect();
        assert!(colors.len() > 10, "渐变必须有多个颜色而非纯色");
        assert!(
            out.shapes.iter().any(
                |s| matches!(&s.shape, egui::Shape::Circle(c) if c.radius > 14.5 && c.fill.a() > 0)
            ),
            "外圈辉光存在"
        );
        let text = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "研" => Some(t),
                _ => None,
            })
            .expect("品牌文字");
        assert!(
            (text.galley.rect.translate(text.pos.to_vec2()).center() - rect.center()).length()
                < 1.0,
            "品牌文字居中"
        );
        assert!(theme::EMBEDDED_CJK_LICENSE.contains("2014-2021 Adobe"));
        assert!(theme::EMBEDDED_CJK_LICENSE.contains("SIL OPEN FONT LICENSE Version 1.1"));
        out.drop_without_applying_deltas();
    });
}

#[test]
fn st_s15_16_boundary_render_evidence() {
    case("ST-S15-16", || {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/research-desktop/screenshots/rd003");
        std::fs::create_dir_all(&dir).expect("创建证据目录");
        assert!(
            theme::MIN_WINDOW.x <= layout::SLIM_MAX_W,
            "原生窗口必须可缩到图标档"
        );
        assert_eq!(theme::MIN_WINDOW, egui::vec2(880.0, 720.0));
        for width in [880.0, 900.0, 901.0, 1200.0, 1201.0] {
            let shapes = frame(width);
            let texts: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some((
                        t.galley.text(),
                        t.galley.rect.translate(t.pos.to_vec2()),
                        s.clip_rect,
                    )),
                    _ => None,
                })
                .collect();
            let sidebar = layout::sidebar_w(layout::plan_for_width(width));
            let bars: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::LineSegment { points, stroke }
                        if points[0].x < sidebar
                            && (points[0].x - points[1].x).abs() < 0.01
                            && stroke.color == theme::ACCENT
                            && (stroke.width - 2.0).abs() < 0.01 =>
                    {
                        Some(points[0].x)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(bars.len(), 1, "只有激活导航含绿色指示条");
            let glows = shapes.iter().filter(|s| matches!(&s.shape,
                egui::Shape::LineSegment { points, stroke } if (points[0].x-bars[0]).abs()<0.01 && (stroke.width-4.0).abs()<0.01)).count();
            assert_eq!(glows, 3, "激活导航必须有三层辉光");

            let brand = texts
                .iter()
                .any(|(t, r, _)| *t == "研序" && r.left() < sidebar);
            assert_eq!(brand, width > 900.0, "品牌名称隐藏断点 {width}");
            for (text, rect, clip) in &texts {
                let visible = rect.intersect(*clip);
                assert!(
                    visible.is_negative() || visible.right() <= width + 1.0,
                    "{width} 可见文本越界：{text}"
                );
            }
            let mut h = harness(width);
            let nav = h.get_by_role_and_label(egui::accesskit::Role::Button, "需求文档");
            assert_eq!(
                nav.rect().width() < 100.0,
                width <= 900.0,
                "窗口 {width}，导航宽 {}，内容矩形 {:?}",
                nav.rect().width(),
                h.ctx.content_rect()
            );
            nav.click_accesskit();
            h.run_steps(3);
            assert_eq!(
                h.state().test_current_route(),
                nautilus_research_desktop::nav::Route::Requirements
            );
            h.render()
                .expect("必须取得 wgpu 离屏截图")
                .save(dir.join(format!("egui-{}.png", width as u32)))
                .expect("保存截图");
        }
    });
}

#[test]
fn ut_gui_05_slim_navigation() {
    case("UT-GUI-05", || {
        let mut h = harness(900.0);
        for route in ALL_ROUTES {
            h.get_by_role_and_label(egui::accesskit::Role::Button, route.title())
                .click_accesskit();
            h.run_steps(3);
            assert_eq!(h.state().test_current_route(), route);
        }
        for s in frame(900.0) {
            if let egui::Shape::Text(t) = s.shape {
                if t.pos.x < 77.0 {
                    assert!(!["研序", "研究工作区", "研究资源"].contains(&t.galley.text()));
                }
            }
        }
    });
}

fn spin(reduce: bool, time: f64) -> Vec<egui::Pos2> {
    let ctx = egui::Context::default();
    let out = ctx.run_ui(Default::default(), |ui| {
        icons::paint_task_spin(
            ui.painter(),
            egui::Rect::from_min_size(egui::pos2(30.0, 30.0), egui::vec2(10.0, 10.0)),
            motion::task_phase(reduce, time),
            theme::ACCENT_TEXT,
        )
    });
    let points = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Path(p) => Some(p.points.clone()),
            _ => None,
        })
        .expect("状态指示器路径");
    out.drop_without_applying_deltas();
    points
}

#[test]
fn ut_gui_06_motion_preference() {
    case("UT-GUI-06", || {
        let root = std::env::temp_dir().join(format!("rd003-profile-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".octos/profiles")).expect("创建配置测试目录");
        let default = root.join(".octos/profiles/octos.json");
        let explicit = root.join("explicit.json");
        std::fs::write(&default, "{}").expect("默认测试配置");
        std::fs::write(&explicit, "{}").expect("显式测试配置");
        assert_eq!(
            test_profile::source(Some(explicit.clone()), Some(root.clone())),
            Some(explicit)
        );
        assert_eq!(
            test_profile::source(Some(root.join("missing")), Some(root)),
            Some(default)
        );
        let _ = test_profile::configured_source();
        assert!(motion::reduce_from_enabled(None));
        assert!(motion::reduce_from_enabled(Some(false)));
        assert!(!motion::reduce_from_enabled(Some(true)));
        assert_eq!(spin(true, 0.0), spin(true, 0.2));
        assert_ne!(spin(false, 0.0), spin(false, 0.2));
        let mut h = harness(1201.0);
        h.get_by_role_and_label(egui::accesskit::Role::Button, "运行记录")
            .click_accesskit();
        h.run_steps(3);
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "减少动态效果")
            .click_accesskit();
        h.run_steps(3);
        assert!(!h.state().reduce_motion, "可访问的显式选择生效");
    });
}
