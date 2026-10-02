//! eframe App 骨架：六页导航 + 玻璃态面板 + 按需重绘。
//!
//! 按需重绘约束（验收红线：不能打爆 CPU）：本骨架不调用
//! `ctx.request_repaint()`，egui 保持事件驱动（无输入不绘制）；
//! L3/L4 的后台任务状态变化时才按需请求重绘。

use egui::{Color32, CornerRadius, FontId, Frame, Margin, Pos2, Stroke};

use crate::layout::{self, LayoutPlan};
use crate::nav::ALL_PAGES;
use crate::session::Session;
use crate::theme;

/// 研究桌面应用。
pub struct ResearchApp {
    /// 会话状态（当前页、滚动、字体提示）。
    pub session: Session,
}

impl ResearchApp {
    /// 新应用（注入字体加载结果提示）。
    pub fn new(font_missing: bool) -> Self {
        Self {
            session: Session::with_font_missing(font_missing),
        }
    }
}

impl eframe::App for ResearchApp {
    /// 根 Ui 渲染（eframe 0.36 模型）：顶栏 + 左导航 + 底状态 + 中栏页面。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 顶栏（56px）：项目占位与窗口说明
        egui::Panel::top("topbar")
            .exact_size(layout::TOPBAR_H)
            .frame(
                Frame::NONE
                    .fill(theme::GLASS)
                    .inner_margin(Margin::symmetric(16, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new("研序 · 策略研究工作区")
                            .font(FontId::proportional(16.0))
                            .color(theme::TEXT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("研究项目 / 未命名研究")
                                .font(FontId::proportional(12.0))
                                .color(theme::TEXT_DIM),
                        );
                    });
                });
            });

        // 左侧导航（180px）：六页；激活项绿色指示条
        egui::Panel::left("nav")
            .exact_size(layout::SIDEBAR_W)
            .frame(
                Frame::NONE
                    .fill(theme::GLASS)
                    .inner_margin(Margin::symmetric(8, 12)),
            )
            .show(ui, |ui| {
                ui.add_space(4.0);
                for page in ALL_PAGES {
                    let active = self.session.page == page;
                    let text =
                        egui::RichText::new(page.title()).font(FontId::proportional(14.0));
                    let label = if active {
                        text.color(theme::ACCENT)
                    } else {
                        text.color(theme::TEXT)
                    };
                    let fill = if active {
                        theme::GLASS_STRONG
                    } else {
                        Color32::TRANSPARENT
                    };
                    let response = ui.add_sized(
                        [ui.available_width(), 34.0],
                        egui::Button::new(label).fill(fill).corner_radius(
                            CornerRadius::same(theme::CARD_ROUNDING),
                        ),
                    );
                    if active {
                        // 激活指示条（左缘绿色竖线）
                        let rect = response.rect;
                        ui.painter().line_segment(
                            [
                                Pos2::new(rect.left(), rect.top() + 6.0),
                                Pos2::new(rect.left(), rect.bottom() - 6.0),
                            ],
                            Stroke::new(3.0, theme::ACCENT),
                        );
                    }
                    if response.clicked() {
                        self.session.navigate(page, 0.0);
                    }
                }
            });

        // 底部任务状态（32px）：占位 + 字体缺失诚实提示
        egui::Panel::bottom("statusbar")
            .exact_size(layout::BOTTOMBAR_H)
            .frame(
                Frame::NONE
                    .fill(theme::GLASS)
                    .inner_margin(Margin::symmetric(16, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        egui::RichText::new("后台任务：无")
                            .font(FontId::monospace(11.0))
                            .color(theme::TEXT_DIM),
                    );
                    if self.session.font_missing {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new("未找到系统中文字体，中文可能显示为方块")
                                    .font(FontId::monospace(11.0))
                                    .color(theme::DANGER),
                            );
                        });
                    }
                });
            });

        // 中栏：当前页占位卡片
        let avail = ui.ctx().content_rect().width();
        let plan = layout::plan_for_width(avail);
        let page = self.session.page;
        egui::Frame::central_panel(ui.style())
            .fill(theme::BASE)
            .inner_margin(Margin::same(16))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("page-scroll")
                    .show(ui, |ui| {
                        egui::Frame::NONE
                            .fill(theme::GLASS_SOFT)
                            .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
                            .stroke(Stroke::new(1.0, theme::STROKE))
                            .inner_margin(Margin::same(24))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.heading(
                                    egui::RichText::new(page.title())
                                        .font(FontId::proportional(22.0))
                                        .color(theme::TEXT),
                                );
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new(page.placeholder())
                                        .font(FontId::proportional(14.0))
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(match plan {
                                        LayoutPlan::ThreeCol => "布局：三栏（宽窗口）",
                                        LayoutPlan::TwoCol => "布局：两栏（窄窗口，右栏收起）",
                                    })
                                    .font(FontId::monospace(11.0))
                                    .color(theme::ACCENT_CYAN),
                                );
                            });
                    });
            });
        // 不调用 request_repaint：事件驱动，静默时零帧（CPU 红线）
    }
}
