//! eframe App：统一工作台三区布局（左侧栏唯一导航 / 主区工作区画布 / 右对话栏）。
//!
//! 高保真对齐 core-05-ai-workspace-prototype.html：左栏 212（窄窗 178）玻璃卡
//! 承载品牌、项目选择与两组 11 路由；主区玻璃大卡内为 65px 顶栏（面包屑 +
//! 演示环境 tag + 运行记录）、工作区画布 + 右对话栏 360（窄窗 320）、28px
//! footer（本地原型声明）；专注模式收起对话栏。旧六页业务动作无损迁入 11 路由。
//!
//! 按需重绘约束（验收红线：不能打爆 CPU）：静默时不请求重绘（零帧）；
//! 仅当存在活跃任务时以 `request_repaint_after(POLL_INTERVAL)` 定时轮询
//! `get_task`（轻量只读），任务推进到终态立即落定并停止轮询。

use egui::{Align, Color32, CornerRadius, FontId, Frame, Layout, Margin, Pos2, Sense, Stroke};

use crate::ai;
use crate::bridge::{CompareForm, DesktopBridge, ImportForm, PlanForm, RunForm, UniverseForm};
use crate::icons;
use crate::layout::{self, LayoutPlan};
use crate::nav::{RESOURCE_GROUP, RESOURCE_ROUTES, Route, WORKSPACE_GROUP, WORKSPACE_ROUTES};
use crate::pipeline::{POLL_INTERVAL, PageState, TaskWatch, terminal_error_text};
use crate::session::Session;
use crate::theme::{self, TagKind};
use crate::workspace::{Role, Workspace};

/// 设计页处理图节点（静态结构说明，非运行数据）。
const NODES: [(&str, &str, &str, &str, u32); 6] = [
    (
        "data",
        "读取数据",
        "信号日可见数据",
        "截止信号日的可见收盘行情 → 价格与成交额样本",
        3,
    ),
    (
        "filter",
        "股票池过滤",
        "流动性与股票池",
        "当期股票池与最低成交额 → 入选或排除原因",
        4,
    ),
    (
        "signal",
        "形成信号",
        "收盘后形成信号",
        "信号日收盘信息 → 目标标的与投入比例",
        5,
    ),
    (
        "size",
        "计算仓位",
        "目标与资金约束",
        "资金、价格、费用与交易单位 → 目标股数与资金检查",
        8,
    ),
    (
        "fill",
        "模拟成交",
        "下一交易日开盘",
        "下一交易日开盘与申请数量 → 成交或拒绝、剩余现金",
        10,
    ),
    (
        "metric",
        "计算指标",
        "现金与持仓估值",
        "逐日现金、持仓与收盘价 → 净值、收益和回撤",
        11,
    ),
];

/// 提交类页面的轮询键。
#[derive(Debug, Clone, Copy, PartialEq)]
enum PageKey {
    Snapshot,
    Universe,
    Plan,
    /// 实验运行（事件调试 / 回测实验共用入口）。
    Run,
}

/// 研究桌面应用（统一工作台）。
pub struct ResearchApp {
    /// 会话状态（当前路由、每路由滚动、字体提示）。
    pub session: Session,
    /// 协调器桥（打开失败时保留错误行，页面提示重试）。
    pub bridge: Option<DesktopBridge>,
    pub bridge_error: Option<String>,

    // ---- 数据中心（旧数据快照页迁入）
    pub import_form: ImportForm,
    pub snapshot_page: PageState<TaskWatch>,
    pub universe_saved: Option<nautilus_research_domain::protocol::UniverseRef>,

    // ---- 股票池
    pub universe_form: UniverseForm,
    pub universe_page: PageState<TaskWatch>,

    // ---- 回测实验（运行参数 + 协调器比较）
    pub run_form: RunForm,
    pub compare_form: CompareForm,
    pub compare_error: Option<String>,
    pub compare_result: Option<nautilus_research_domain::protocol::Comparison>,

    // ---- 交易计划（协调器生产对接面板）
    pub plan_form: PlanForm,
    pub plan_page: PageState<TaskWatch>,
    pub plan_exported: Option<String>,

    // ---- 研究工作区状态机（S17~S20：会话态随应用存活，切路由/切项目不丢失）
    pub workspace: Workspace,
    pub ai_input: String,
    pub ai_notice: Option<String>,
    // 需求文档：需求确认表单
    pub ai_req_text: String,
    pub ai_req_acceptance: String,
    pub ai_req_alloc: f64,
    pub ai_req_min_amount: String,
    // 策略设计：设计说明；策略开发：代码草稿
    pub ai_design_note: String,
    pub ai_code_source: String,
    // 实验任务与当前实验引用
    pub ai_run: PageState<TaskWatch>,
    pub ai_experiment: Option<usize>,
    // 计划桥：交易日/持仓 JSON/核对结果/导出产物
    pub ai_plan_trade_date: String,
    pub ai_plan_json: String,
    pub ai_plan_issues: Option<Vec<String>>,
    pub ai_plan_csv: Option<String>,

    // ---- 呈现层状态（本批新增）
    /// 专注模式（收起右对话栏）。
    pub focus: bool,
    /// 运行记录浮层开关。
    pub show_logs: bool,
    /// 运行记录（任务动作、依据和产物；不含模型内部推理）。
    pub logs: Vec<String>,
    /// 对话产物卡（项目 ID, 消息序号 → 目标路由）。
    pub chat_cards: Vec<(usize, usize, Route)>,
    /// 新建项目浮层。
    pub new_project_open: bool,
    pub new_project_name: String,
    /// 设计页处理图 tab：true = 流程图，false = 时序图。
    pub design_tab_flow: bool,
    /// 设计页选中节点（NODES 下标）。
    pub design_node: usize,
    /// 调试页比较的两个实验 ID。
    pub debug_cmp: [usize; 2],
    /// 版本只读查看浮层（版本 ID, 源码）。
    pub view_source: Option<(usize, String)>,
    /// 对话区回底标记（新消息后滚动到底部）。
    chat_pin: bool,
    /// 上一帧路由（用于一次性恢复滚动位置）。
    last_route: Option<Route>,
}

impl ResearchApp {
    /// 新应用（注入字体加载结果提示；协调器桥由 `attach_bridge` 建立）。
    pub fn new(font_missing: bool) -> Self {
        Self {
            session: Session::with_font_missing(font_missing),
            bridge: None,
            bridge_error: None,
            import_form: ImportForm::default(),
            snapshot_page: PageState::default(),
            universe_saved: None,
            universe_form: UniverseForm {
                rule_text: r#"{"op":"and","children":[{"field":"close","op":"gte","value":"0"}]}"#
                    .into(),
                as_of: String::new(),
                strict: true,
                fixed_membership: true,
                ignore_missing: false,
            },
            universe_page: PageState::default(),
            run_form: RunForm::default(),
            compare_form: CompareForm::default(),
            compare_error: None,
            compare_result: None,
            plan_form: PlanForm::default(),
            plan_page: PageState::default(),
            plan_exported: None,
            workspace: Workspace::new(),
            ai_input: String::new(),
            ai_notice: None,
            ai_req_text: String::new(),
            ai_req_acceptance: String::new(),
            ai_req_alloc: 30.0,
            ai_req_min_amount: "1000000".into(),
            ai_design_note: String::new(),
            ai_code_source: String::new(),
            ai_run: PageState::default(),
            ai_experiment: None,
            ai_plan_trade_date: String::new(),
            ai_plan_json: String::new(),
            ai_plan_issues: None,
            ai_plan_csv: None,
            focus: false,
            show_logs: false,
            logs: Vec::new(),
            // 欢迎消息挂需求草稿产物卡（与原型一致）
            chat_cards: vec![(1, 0, Route::Requirements)],
            new_project_open: false,
            new_project_name: String::new(),
            design_tab_flow: true,
            design_node: 3,
            debug_cmp: [1, 2],
            view_source: None,
            chat_pin: true,
            last_route: None,
        }
    }

    /// 建立协调器桥（工作区路径：环境变量 RESEARCH_WORKSPACE 优先）。
    pub fn attach_bridge(&mut self, workspace: std::path::PathBuf) {
        match DesktopBridge::open(workspace) {
            Ok(b) => {
                self.bridge_error = None;
                self.bridge = Some(b);
            }
            Err(e) => {
                self.bridge_error = Some(format!("工作区打开失败：{}", e.message));
                self.bridge = None;
            }
        }
    }
}

/// 概览下一步推断（纯函数，与原型 nextStep 同源的桌面口径）：
/// 需求 → 设计 → 版本 → 实验 → 报告 → 计划 → 核对 → 导出，逐级给出理由。
pub fn next_step(w: &Workspace) -> (String, Route, String) {
    let p = w.current();
    let Some(req) = p.active_req else {
        return (
            "确认研究需求".into(),
            Route::Requirements,
            "先确认研究目标与验收标准".into(),
        );
    };
    let design_ok = p
        .active_design
        .is_some_and(|d| p.designs.get(d - 1).is_some_and(|x| x.req_id == req));
    if !design_ok {
        return (
            "保存当前设计".into(),
            Route::Design,
            "设计缺失或引用旧需求，请同步后保存".into(),
        );
    }
    let vid = p.active_version;
    if vid.is_none_or(|v| !w.version_fresh(v)) {
        return (
            "保存当前策略版本".into(),
            Route::Develop,
            "策略尚未保存或上游已变化，请检查并冻结新版本".into(),
        );
    }
    let vid = vid.unwrap_or_default();
    let run = p
        .experiments
        .iter()
        .filter(|e| e.version_id == vid && e.stamp == p.revision)
        .map(|e| e.id)
        .max();
    let Some(run) = run else {
        return (
            "运行当前版本实验".into(),
            Route::Experiments,
            format!("当前 v{vid} 尚无实验；历史实验不代替当前证据"),
        );
    };
    let report = p.report.as_ref();
    let report_current = report.is_some_and(|r| r.version_id == vid && w.version_fresh(vid));
    if !report_current {
        return (
            "生成验证报告".into(),
            Route::Validate,
            format!("当前实验 E{run} 已完成，等待证据检查"),
        );
    }
    if !report.is_some_and(|r| r.demo_pass()) {
        return (
            "排查失败证据".into(),
            Route::Validate,
            "演示检查失败，从报告打开关联实验定位原因".into(),
        );
    }
    let Some(t) = &p.plan else {
        return (
            "生成演示计划".into(),
            Route::Plan,
            "当前演示检查通过，请使用独立账户和计划快照生成草稿".into(),
        );
    };
    if t.version_id != vid {
        return (
            "生成演示计划".into(),
            Route::Plan,
            "计划引用旧版本，请使用当前版本重新生成草稿".into(),
        );
    }
    if !t.checked {
        return (
            "核对交易计划".into(),
            Route::Plan,
            "草稿尚未核对现金、可卖数量与数据时效".into(),
        );
    }
    (
        "确认导出交易清单".into(),
        Route::Plan,
        "计划核对通过；正式策略验证仍为证据不足".into(),
    )
}

impl eframe::App for ResearchApp {
    /// 根 Ui 渲染（eframe 0.36 模型）：环境柔光 + 左侧栏卡 + 主区卡（顶栏/画布+对话/footer）。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 双环境柔光铺满根矩形：左上绿 / 右下青（面板玻璃半透明，柔光透出）
        theme::paint_ambient(ui);
        // 水平间距手动管理（卡片间隙 = APP_GAP）
        ui.spacing_mut().item_spacing.x = 0.0;

        // 首帧守卫：WSLg 等环境 winit 首帧可能给出远小于请求值的窗口尺寸
        // （实测约 260×267），此时宽度减侧栏与对话栏必为负——跳过本帧等尺寸就绪
        let root = ui.max_rect();
        if !layout::frame_ready(root.width(), root.height()) {
            ui.ctx().request_repaint();
            return;
        }

        let plan = layout::plan_for_width(ui.max_rect().width());
        let outer = ui.max_rect().shrink(layout::APP_GAP);
        ui.allocate_ui(sz(outer.width(), outer.height()), |ui| {
            ui.horizontal(|ui| {
                self.render_sidebar(ui, plan);
                ui.add_space(layout::APP_GAP);
                self.render_main(ui, plan);
            });
        });

        // 轮询所有提交类任务：get_task → 状态机推进（终态落定并停止轮询）
        for key in [
            PageKey::Snapshot,
            PageKey::Universe,
            PageKey::Plan,
            PageKey::Run,
        ] {
            self.poll(key);
        }
        // 按需重绘：有活跃任务才定时轮询；否则事件驱动，静默零帧（CPU 红线）
        if self.active_task_count() > 0 {
            ui.ctx().request_repaint_after(POLL_INTERVAL);
        }
    }
}

// ================================================================ 布局骨架

impl ResearchApp {
    /// 左侧栏（唯一导航）：品牌 + 项目选择/新建 + 两组 11 路由 + 底部离线声明。
    fn render_sidebar(&mut self, ui: &mut egui::Ui, plan: LayoutPlan) {
        Frame::NONE
            .fill(theme::GLASS)
            .corner_radius(CornerRadius::same(16))
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(Margin {
                left: 14,
                right: 14,
                top: 22,
                bottom: 16,
            })
            .show(ui, |ui| {
                let w = layout::sidebar_w(plan) - 28.0;
                ui.set_min_size(egui::vec2(w, ui.available_height()));
                ui.set_max_width(w);

                // 品牌：研 + 研序 / 策略研究工作区
                ui.horizontal(|ui| {
                    let mark = Frame::NONE
                        .fill(theme::ACCENT)
                        .corner_radius(CornerRadius::same(9))
                        .inner_margin(Margin::symmetric(6, 3));
                    mark.show(ui, |ui| {
                        ui.set_min_size(egui::vec2(17.0, 23.0));
                        ui.centered_and_justified(|ui| {
                            ui.label(lbl("研", 19.0, theme::BASE));
                        });
                    });
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(lbl("研序", 20.0, theme::TEXT));
                        ui.label(lbl("策略研究工作区", 9.0, theme::MUTED));
                    });
                });
                ui.add_space(18.0);

                // 项目选择 + 新建（项目隔离：切回时版本与消息保留）
                theme::section_label(ui, "当前项目");
                let names: Vec<(usize, String)> = self
                    .workspace
                    .projects
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (i, p.name.clone()))
                    .collect();
                let active = self.workspace.active;
                egui::ComboBox::from_id_salt("project-select")
                    .selected_text(names[active].1.clone())
                    .width(w)
                    .show_ui(ui, |ui| {
                        for (i, name) in &names {
                            if ui.selectable_label(*i == active, name.clone()).clicked() {
                                self.workspace.switch_to(*i);
                            }
                        }
                    });
                ui.add_space(6.0);
                if ui
                    .add_sized([w, 26.0], theme::ghost_button("＋ 新建研究项目"))
                    .clicked()
                {
                    self.new_project_open = true;
                }
                ui.add_space(18.0);

                // 研究工作区 8 路由
                theme::section_label(ui, WORKSPACE_GROUP);
                for r in WORKSPACE_ROUTES {
                    self.nav_item(ui, r, w);
                }
                ui.add_space(14.0);
                // 研究资源 3 路由
                theme::section_label(ui, RESOURCE_GROUP);
                for r in RESOURCE_ROUTES {
                    self.nav_item(ui, r, w);
                }

                // 底部：本地声明（含字体缺失诚实提示）
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.separator();
                    ui.add_space(10.0);
                    if self.session.font_missing {
                        ui.label(lbl(
                            "未找到系统中文字体，中文可能显示为方块",
                            9.0,
                            theme::RED,
                        ));
                        ui.add_space(4.0);
                    }
                    ui.horizontal(|ui| {
                        let avatar = Frame::NONE
                            .fill(theme::GLASS_STRONG)
                            .corner_radius(CornerRadius::same(14))
                            .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                            .inner_margin(Margin::symmetric(5, 5));
                        avatar.show(ui, |ui| {
                            ui.set_min_size(egui::vec2(17.0, 17.0));
                            ui.centered_and_justified(|ui| {
                                ui.label(lbl("本地", 10.0, theme::TEXT2));
                            });
                        });
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.label(lbl("个人研究空间", 11.0, theme::TEXT2));
                            ui.label(lbl("离线 · 无自动下单", 9.0, theme::MUTED));
                        });
                    });
                });
            });
    }

    /// 导航项：激活 = 绿柔底 + 主色描边 + 左缘 2px 指示条；回测实验附实验计数。
    fn nav_item(&mut self, ui: &mut egui::Ui, route: Route, w: f32) {
        let active = self.session.route == route;
        let count = (route == Route::Experiments)
            .then(|| self.workspace.current().experiments.len())
            .filter(|n| *n > 0);
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 30.0), Sense::click());
        let fill = if active {
            Color32::from_rgba_premultiplied(5, 20, 11, 20)
        } else if resp.hovered() {
            Color32::from_rgba_premultiplied(250, 250, 250, 10)
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if active {
            Stroke::new(1.0, Color32::from_rgba_premultiplied(8, 26, 15, 61))
        } else {
            Stroke::NONE
        };
        ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
        if active {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(7),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        // 16px 图标（原型 .nav button svg：激活态取 --accent-text，非激活随文字 muted）
        let icon_rect = egui::Rect::from_center_size(
            Pos2::new(rect.left() + 11.0 + 8.0, rect.center().y),
            egui::Vec2::splat(16.0),
        );
        icons::paint_icon(
            ui.painter(),
            icon_rect,
            icons::for_route(route),
            if active {
                theme::ACCENT_TEXT
            } else {
                theme::MUTED
            },
        );
        let tx = rect.left() + 11.0 + 24.0;
        let cy = rect.center().y;
        ui.painter().text(
            Pos2::new(tx, cy),
            egui::Align2::LEFT_CENTER,
            route.title(),
            FontId::proportional(12.0),
            if active { theme::TEXT } else { theme::MUTED },
        );
        if let Some(n) = count {
            ui.painter().text(
                Pos2::new(rect.right() - 10.0, cy),
                egui::Align2::RIGHT_CENTER,
                n.to_string(),
                FontId::monospace(10.0),
                theme::FAINT,
            );
        }
        if active {
            // 左缘 2px 绿指示条（原型 .nav button.active:before）
            ui.painter().line_segment(
                [
                    Pos2::new(rect.left(), cy - 9.0),
                    Pos2::new(rect.left(), cy + 9.0),
                ],
                Stroke::new(2.0, theme::ACCENT),
            );
        }
        if resp.clicked() {
            let from = self.session.route;
            self.session.navigate(route, self.session.scroll_of(from));
        }
    }

    /// 主区大卡：65px 顶栏 +（工作区画布 | 右对话栏）+ 28px footer。
    fn render_main(&mut self, ui: &mut egui::Ui, plan: LayoutPlan) {
        Frame::NONE
            .fill(theme::MAIN_BG)
            .corner_radius(CornerRadius::same(16))
            .stroke(Stroke::new(1.0, theme::BORDER))
            .show(ui, |ui| {
                let w = ui.available_width();
                let h = ui.available_height();
                ui.set_min_size(egui::vec2(w, h));
                ui.set_max_width(w);
                ui.spacing_mut().item_spacing.y = 0.0;

                self.render_topbar(ui, w);
                self.render_body(ui, w, h - layout::TOPBAR_H - layout::FOOTER_H, plan);
                self.render_footer(ui, w);
            });

        // 浮层：运行记录 / 版本查看 / 新建项目
        self.render_windows(ui.ctx());
    }

    /// 顶栏：面包屑（研究项目 / 项目名）+ 演示环境 tag + 运行记录按钮。
    fn render_topbar(&mut self, ui: &mut egui::Ui, w: f32) {
        let project = self.workspace.current().name.clone();
        let resp = ui
            .allocate_ui(sz(w, layout::TOPBAR_H), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(25.0);
                    ui.label(lbl("研究项目", 12.0, theme::MUTED));
                    ui.add_space(10.0);
                    ui.label(lbl("/", 12.0, theme::FAINT));
                    ui.add_space(10.0);
                    ui.label(lbl(project, 12.0, theme::TEXT2));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(25.0);
                        if ui.add(theme::ghost_button("运行记录")).clicked() {
                            self.show_logs = true;
                        }
                        ui.add_space(8.0);
                        theme::tag_ui(ui, "● 演示环境", TagKind::Accent);
                    });
                });
            })
            .response;
        let r = resp.rect;
        ui.painter().line_segment(
            [
                Pos2::new(r.left(), r.bottom()),
                Pos2::new(r.right(), r.bottom()),
            ],
            Stroke::new(1.0, theme::BORDER),
        );
    }

    /// footer：本地原型声明 + 溯源声明。
    fn render_footer(&mut self, ui: &mut egui::Ui, w: f32) {
        let resp = ui
            .allocate_ui(sz(w, layout::FOOTER_H), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(lbl(
                        "● 本地原型 · 合成样本 SYN-202601 · 刷新重置，可导出产物",
                        9.0,
                        theme::FAINT,
                    ));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(20.0);
                        ui.label(lbl("所有结果可追溯至输入与版本", 9.0, theme::FAINT));
                    });
                });
            })
            .response;
        let r = resp.rect;
        ui.painter().line_segment(
            [Pos2::new(r.left(), r.top()), Pos2::new(r.right(), r.top())],
            Stroke::new(1.0, theme::BORDER),
        );
    }

    /// 主体：工作区画布（含 workspace-head）+ 右对话栏（专注模式收起）。
    fn render_body(&mut self, ui: &mut egui::Ui, w: f32, h: f32, plan: LayoutPlan) {
        ui.allocate_ui(sz(w, h), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let chat_w = if self.focus {
                    0.0
                } else {
                    layout::chat_w(plan)
                };
                self.render_canvas(ui, w - chat_w, h);
                if !self.focus {
                    self.render_chat(ui, chat_w, h);
                }
            });
        });
    }

    /// 工作区画布：workspace-head（◇ 项目产物 + 版本 tag + 专注开关）+ 路由页面。
    fn render_canvas(&mut self, ui: &mut egui::Ui, w: f32, h: f32) {
        ui.allocate_ui(sz(w, h), |ui| {
            ui.set_min_width(w);
            // workspace-head
            let version_tag = {
                let p = self.workspace.current();
                format!(
                    "R{} / D{} / v{}",
                    p.active_req
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "—".into()),
                    p.active_design
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "—".into()),
                    p.active_version
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "—".into()),
                )
            };
            let head = ui
                .allocate_ui(sz(w, 44.0), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.add_space(23.0);
                        ui.label(lbl("◇", 12.0, theme::ACCENT_TEXT));
                        ui.add_space(8.0);
                        ui.label(lbl("项目产物", 12.0, theme::TEXT2));
                        ui.add_space(8.0);
                        theme::tag_ui(ui, &version_tag, TagKind::Neutral);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(23.0);
                            let label = if self.focus {
                                "返回对话 ⤡"
                            } else {
                                "展开工作区 ⤢"
                            };
                            if ui.add(theme::ghost_button(label)).clicked() {
                                self.focus = !self.focus;
                            }
                        });
                    });
                })
                .response;
            let hr = head.rect;
            ui.painter().line_segment(
                [
                    Pos2::new(hr.left(), hr.bottom()),
                    Pos2::new(hr.right(), hr.bottom()),
                ],
                Stroke::new(1.0, theme::BORDER),
            );

            // 画布滚动：切路由一次性恢复保存的滚动偏移，每帧回写
            let route = self.session.route;
            let restore = self.last_route != Some(route);
            self.last_route = Some(route);
            let mut sa = egui::ScrollArea::vertical().id_salt("canvas-scroll");
            if restore {
                sa = sa.vertical_scroll_offset(self.session.scroll_of(route));
            }
            let out = sa.show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.add_space(25.0);
                    ui.vertical(|ui| {
                        self.render_notice(ui);
                        self.render_page(ui, route);
                        ui.add_space(24.0);
                    });
                });
            });
            self.session.record_scroll(route, out.state.offset.y);
        });
    }

    /// 页面分发（11 路由）。
    fn render_page(&mut self, ui: &mut egui::Ui, route: Route) {
        match route {
            Route::Overview => self.render_overview(ui),
            Route::Requirements => self.render_requirements(ui),
            Route::Design => self.render_design(ui),
            Route::Develop => self.render_develop(ui),
            Route::Debug => self.render_debug(ui),
            Route::Experiments => self.render_experiments(ui),
            Route::Validate => self.render_validate(ui),
            Route::Plan => self.render_plan_page(ui),
            Route::Data => self.render_data(ui),
            Route::Pool => self.render_pool(ui),
            Route::Library => self.render_library(ui),
        }
    }
}

// ================================================================ 右对话栏

impl ResearchApp {
    /// 右对话栏：研究助手头部 + 消息流（含产物卡）+ 任务条 + 建议与输入。
    fn render_chat(&mut self, ui: &mut egui::Ui, w: f32, h: f32) {
        ui.allocate_ui(sz(w, h), |ui| {
            ui.set_min_width(w);
            let left_line = ui.painter().line_segment(
                [
                    Pos2::new(ui.min_rect().left(), ui.min_rect().top()),
                    Pos2::new(ui.min_rect().left(), ui.min_rect().bottom()),
                ],
                Stroke::new(1.0, theme::BORDER),
            );
            let _ = left_line;

            // 头部：✧ 研究助手 / 从一个想法，到可追溯的计划 + 预设对话 tag
            ui.allocate_ui(sz(w, 64.0), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(22.0);
                    Frame::NONE
                        .fill(Color32::from_rgba_premultiplied(5, 20, 11, 20))
                        .corner_radius(CornerRadius::same(8))
                        .stroke(Stroke::new(
                            1.0,
                            Color32::from_rgba_premultiplied(8, 26, 15, 51),
                        ))
                        .inner_margin(Margin::symmetric(7, 5))
                        .show(ui, |ui| {
                            ui.label(lbl("✧", 13.0, theme::ACCENT_TEXT));
                        });
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(lbl("研究助手", 13.0, theme::TEXT));
                        ui.label(lbl("从一个想法，到可追溯的计划", 9.0, theme::MUTED));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(22.0);
                        theme::tag_ui(ui, "预设对话", TagKind::Neutral);
                    });
                });
            });

            // 消息流（最近 20 条，避免长会话拖慢帧；完整历史保留在会话态）
            let msgs: Vec<(bool, String, Option<Route>)> = {
                let p = self.workspace.current();
                let start = p.messages.len().saturating_sub(20);
                p.messages[start..]
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        (
                            m.role == Role::User,
                            m.text.clone(),
                            self.chat_card(p.id, start + i),
                        )
                    })
                    .collect()
            };
            egui::ScrollArea::vertical()
                .id_salt("chat-scroll")
                .auto_shrink(false)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        ui.vertical(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.centered_and_justified(|ui| {
                                ui.label(lbl("项目对话 · 上下文与产物持续关联", 9.0, theme::FAINT));
                            });
                            for (is_user, text, card) in &msgs {
                                self.render_message(ui, *is_user, text, *card);
                            }
                            if self.chat_pin {
                                ui.scroll_to_cursor(Some(Align::BOTTOM));
                            }
                        });
                    });
                    ui.add_space(16.0);
                });
            self.chat_pin = false;

            // 任务条（活跃任务：取消入口）
            let task = self
                .active_task()
                .map(|(id, label, key)| (id.to_string(), label, key));
            if let Some((task_id, label, key)) = task {
                ui.allocate_ui(sz(w, 40.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.set_min_width(ui.available_width());
                        Frame::NONE
                            .fill(Color32::from_rgba_premultiplied(5, 20, 11, 15))
                            .corner_radius(CornerRadius::same(8))
                            .stroke(Stroke::new(
                                1.0,
                                Color32::from_rgba_premultiplied(8, 26, 15, 51),
                            ))
                            .inner_margin(Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.label(mono(ellipsis(&label, 18), 11.0, theme::ACCENT_TEXT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add(theme::ghost_button("取消")).clicked() {
                                        self.do_cancel(task_id.to_string(), key);
                                    }
                                });
                            });
                    });
                });
            }

            // 输入区
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width());
                    // 建议 chips（随研究阶段推进）
                    ui.horizontal_wrapped(|ui| {
                        for s in self.suggestions() {
                            if ui
                                .add(
                                    egui::Button::new(lbl(s, 10.0, theme::MUTED))
                                        .fill(theme::GLASS)
                                        .corner_radius(CornerRadius::same(7)),
                                )
                                .clicked()
                            {
                                self.do_ai_send_text(s.to_string());
                            }
                        }
                    });
                    ui.add_space(8.0);
                    // compose-box：输入 + ↵ 发送
                    Frame::NONE
                        .fill(theme::GLASS_STRONG)
                        .corner_radius(CornerRadius::same(12))
                        .stroke(Stroke::new(
                            1.0,
                            Color32::from_rgba_premultiplied(8, 26, 15, 77),
                        ))
                        .inner_margin(Margin::same(11))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            let mut input = self.ai_input.clone();
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut input)
                                    .hint_text("描述你的研究想法，或让助手解释当前结果…")
                                    .desired_width(ui.available_width()),
                            );
                            if resp.changed() {
                                self.ai_input = input;
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(lbl("↵ 发送 · Shift + ↵ 换行", 9.0, theme::FAINT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let send = ui
                                        .add(theme::primary_button("↑"))
                                        .on_hover_text("发送消息");
                                    let enter = resp.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if send.clicked() || enter {
                                        let text = std::mem::take(&mut self.ai_input);
                                        self.do_ai_send_text(text);
                                    }
                                });
                            });
                        });
                    ui.add_space(6.0);
                    ui.centered_and_justified(|ui| {
                        ui.label(lbl(
                            "离线预设交互 · 计算来自合成样本 · 未接入模型",
                            9.0,
                            theme::FAINT,
                        ));
                    });
                });
            });
        });
    }

    /// 单条消息：meta + 气泡 + 产物卡（可点击跳转路由）。
    fn render_message(
        &mut self,
        ui: &mut egui::Ui,
        is_user: bool,
        text: &str,
        card: Option<Route>,
    ) {
        ui.add_space(10.0);
        if is_user {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.label(lbl("你", 10.0, theme::FAINT));
            });
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                Frame::NONE
                    .fill(theme::GLASS_SOFT)
                    .corner_radius(CornerRadius {
                        nw: 10,
                        ne: 10,
                        sw: 10,
                        se: 2,
                    })
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .inner_margin(Margin::symmetric(13, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(360.0));
                        ui.label(lbl(text, 12.0, theme::TEXT2));
                    });
            });
        } else {
            ui.label(lbl("✧ 研究助手", 10.0, theme::ACCENT_TEXT));
            ui.label(lbl(text, 12.0, theme::TEXT2));
        }
        if let Some(route) = card {
            ui.add_space(4.0);
            let title = route.title();
            let inner = ui
                .horizontal(|ui| {
                    ui.set_min_width(ui.available_width());
                    Frame::NONE
                        .fill(Color32::from_rgba_premultiplied(6, 21, 12, 26))
                        .corner_radius(CornerRadius::same(9))
                        .stroke(Stroke::new(
                            1.0,
                            Color32::from_rgba_premultiplied(8, 26, 15, 56),
                        ))
                        .inner_margin(Margin::symmetric(13, 10))
                        .show(ui, |ui| {
                            ui.label(lbl(title, 12.0, theme::TEXT));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(lbl("→", 12.0, theme::ACCENT_TEXT));
                            });
                        });
                })
                .response;
            let resp = inner.interact(Sense::click());
            if resp.clicked() {
                let from = self.session.route;
                self.session.navigate(route, self.session.scroll_of(from));
            }
        }
        ui.add_space(12.0);
    }

    /// 产物卡查找（项目 ID + 消息序号 → 目标路由）。
    fn chat_card(&self, project_id: usize, msg_index: usize) -> Option<Route> {
        self.chat_cards
            .iter()
            .rev()
            .find(|(pid, idx, _)| *pid == project_id && *idx == msg_index)
            .map(|(_, _, r)| *r)
    }

    /// 建议文案（随研究阶段：无需求 → 需求草稿；无设计 → 设计；否则代码/实验/报告）。
    fn suggestions(&self) -> Vec<&'static str> {
        let p = self.workspace.current();
        if p.active_req.is_none() {
            vec!["生成需求草稿", "解释验证区别"]
        } else if p.active_design.is_none() {
            vec!["生成策略设计", "解释验证区别"]
        } else {
            vec!["生成策略代码", "运行回测实验", "生成验证报告"]
        }
    }
}

// ================================================================ 11 路由页面

impl ResearchApp {
    /// 通用提示行（错误红 + 可关闭）。
    fn render_notice(&mut self, ui: &mut egui::Ui) {
        if let Some(n) = self.ai_notice.clone() {
            Frame::NONE
                .fill(Color32::from_rgba_premultiplied(38, 12, 12, 26))
                .corner_radius(CornerRadius::same(8))
                .stroke(Stroke::new(
                    1.0,
                    Color32::from_rgba_premultiplied(38, 12, 12, 71),
                ))
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(lbl(&n, 11.0, theme::RED));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(theme::ghost_button("知道了")).clicked() {
                                self.ai_notice = None;
                            }
                        });
                    });
                });
            ui.add_space(12.0);
        }
    }

    /// 警示横幅（琥珀）。
    fn warn_banner(ui: &mut egui::Ui, text: &str) {
        Frame::NONE
            .fill(Color32::from_rgba_premultiplied(39, 28, 5, 15))
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(
                1.0,
                Color32::from_rgba_premultiplied(39, 28, 5, 64),
            ))
            .inner_margin(Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(lbl(text, 11.0, theme::AMBER));
            });
        ui.add_space(12.0);
    }

    // ---- 项目概览 ------------------------------------------------------------
    fn render_overview(&mut self, ui: &mut egui::Ui) {
        let waiting = self.active_task().map(|(_, label, _)| label.to_string());
        let (next_label, next_route, next_reason) = next_step(&self.workspace);
        let (next_label, next_route, next_reason) = match waiting {
            Some(label) => (
                "等待任务完成".to_string(),
                self.session.route,
                format!("{label}正在执行，可在对话区取消"),
            ),
            None => (next_label, next_route, next_reason),
        };
        let (project_name, versions_len, exps_len) = {
            let p = self.workspace.current();
            (p.name.clone(), p.versions.len(), p.experiments.len())
        };
        page_header(
            ui,
            "项目 / 研究概览",
            "让每一步研究，都有依据。",
            "需求、策略与实验在这里连接成一条完整的研究路径。",
        );

        // hero（原型 .hero：110° 线性渐变底 + 右侧 hero-orbit 双环）
        Frame::NONE
            .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                let hero_rect = ui.max_rect();
                // 110° 渐变底（左上亮 → 右下暗）
                {
                    let mut mesh = egui::Mesh::default();
                    mesh.colored_vertex(hero_rect.left_top(), icons::HERO_GRADIENT_FROM);
                    mesh.colored_vertex(hero_rect.right_top(), icons::HERO_GRADIENT_TO);
                    mesh.colored_vertex(hero_rect.right_bottom(), icons::HERO_GRADIENT_TO);
                    mesh.colored_vertex(hero_rect.left_bottom(), icons::HERO_GRADIENT_TO);
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    ui.painter().add(egui::Shape::mesh(mesh));
                }
                // hero-orbit：右上 110px 双环（窄窗 70px 降透明度）
                let narrow = layout::plan_for_width(ui.max_rect().width()) == LayoutPlan::Compact;
                let side = if narrow { 70.0 } else { 110.0 };
                let orbit_rect = egui::Rect::from_center_size(
                    Pos2::new(
                        hero_rect.right() - 20.0 - side / 2.0,
                        hero_rect.top() + 30.0 + side / 2.0,
                    ),
                    egui::Vec2::splat(side),
                );
                icons::paint_hero_orbit(ui.painter(), orbit_rect, narrow);
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    theme::tag_ui(ui, "A股 · 日线", TagKind::Accent);
                    theme::tag_ui(
                        ui,
                        &format!("项目 {:02}", self.workspace.current().id),
                        TagKind::Neutral,
                    );
                });
                ui.add_space(10.0);
                ui.label(lbl(&project_name, 20.0, theme::TEXT));
                ui.add_space(6.0);
                ui.label(lbl(&next_reason, 12.0, theme::MUTED));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button(&next_label)).clicked() {
                        let from = self.session.route;
                        self.session
                            .navigate(next_route, self.session.scroll_of(from));
                    }
                    ui.add_space(8.0);
                    if ui.add(theme::ghost_button("了解验证流程")).clicked() {
                        let from = self.session.route;
                        self.session
                            .navigate(Route::Validate, self.session.scroll_of(from));
                    }
                });
            });
        ui.add_space(12.0);

        // 指标卡 ×3
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 24.0) / 3.0;
            for (top, value, bottom) in [
                (
                    "研究阶段",
                    next_route.title().to_string(),
                    "逐步构建研究证据".to_string(),
                ),
                (
                    "策略版本",
                    format!("{versions_len:02}"),
                    "保存后不可覆盖".to_string(),
                ),
                (
                    "已完成实验",
                    format!("{exps_len:02}"),
                    "输入与结果可追溯".to_string(),
                ),
            ] {
                ui.allocate_ui(sz(w, 0.0), |ui| {
                    Frame::NONE
                        .fill(Color32::from_rgba_premultiplied(250, 250, 250, 8))
                        .corner_radius(CornerRadius::same(10))
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .inner_margin(Margin::same(17))
                        .show(ui, |ui| {
                            ui.label(lbl(top, 10.0, theme::MUTED));
                            ui.add_space(8.0);
                            ui.label(mono(value, 23.0, theme::TEXT));
                            ui.add_space(4.0);
                            ui.label(lbl(bottom, 10.0, theme::MUTED));
                        });
                });
                ui.add_space(12.0);
            }
        });
        ui.add_space(4.0);

        // 研究路径（五阶段，可点击跳转）
        let stages: Vec<(Route, String, String, String, TagKind)> = {
            let p = self.workspace.current();
            let req_state = p
                .active_req
                .map(|id| format!("R{id} 已确认"))
                .unwrap_or_else(|| "待确认".into());
            let design_state = match p.active_design {
                Some(d) => {
                    let fresh = p
                        .active_req
                        .is_some_and(|r| p.designs.get(d - 1).is_some_and(|x| x.req_id == r));
                    format!("D{d} {}", if fresh { "已保存" } else { "已过期" })
                }
                None => "待设计".into(),
            };
            let dev_state = match p.active_version {
                Some(v) => {
                    let fresh = self.workspace.version_fresh(v);
                    format!("v{v} {}", if fresh { "可运行" } else { "已过期" })
                }
                None => "待开发".into(),
            };
            let exp_state = match p.active_version {
                Some(v) if p.experiments.iter().any(|e| e.version_id == v) => {
                    "当前版本已运行".into()
                }
                _ => "当前版本待运行".into(),
            };
            let val_state = if report_ready(&self.workspace) {
                "演示检查通过".into()
            } else {
                "待验证".into()
            };
            vec![
                (
                    Route::Requirements,
                    "需求与研究假设".into(),
                    "定义目标、数据范围与验收标准".into(),
                    req_state,
                    if p.active_req.is_some() {
                        TagKind::Accent
                    } else {
                        TagKind::Neutral
                    },
                ),
                (
                    Route::Design,
                    "设计与处理逻辑".into(),
                    "系统流程图 · 调仓周期时序图".into(),
                    design_state,
                    if p.active_design.is_some() {
                        TagKind::Accent
                    } else {
                        TagKind::Neutral
                    },
                ),
                (
                    Route::Develop,
                    "策略实现".into(),
                    "编辑代码、检查并保存不可变版本".into(),
                    dev_state,
                    if p.active_version.is_some() {
                        TagKind::Accent
                    } else {
                        TagKind::Neutral
                    },
                ),
                (
                    Route::Experiments,
                    "调试与回测实验".into(),
                    "解释事件成因，比较历史模拟结果".into(),
                    exp_state,
                    TagKind::Neutral,
                ),
                (
                    Route::Validate,
                    "验证与交易计划".into(),
                    "证据检查、账户核对与人工清单".into(),
                    val_state,
                    if report_ready(&self.workspace) {
                        TagKind::Accent
                    } else {
                        TagKind::Neutral
                    },
                ),
            ]
        };
        panel(
            ui,
            "研究路径",
            Some(("同一项目 · 全程关联", TagKind::Neutral)),
            |ui| {
                for (i, (route, title, desc, state, kind)) in stages.iter().enumerate() {
                    let inner = ui
                        .horizontal(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.add_space(2.0);
                            // 步号圆圈
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(25.0, 25.0), Sense::hover());
                            let c = rect.center();
                            ui.painter().circle_stroke(
                                c,
                                12.0,
                                Stroke::new(1.0, Color32::from_rgba_premultiplied(8, 26, 15, 64)),
                            );
                            ui.painter().text(
                                c,
                                ALIGN2_CENTER,
                                format!("{:02}", i + 1),
                                FontId::monospace(10.0),
                                theme::ACCENT_TEXT,
                            );
                            ui.add_space(8.0);
                            ui.vertical(|ui| {
                                ui.label(lbl(title, 12.0, theme::TEXT));
                                ui.label(lbl(desc, 10.0, theme::MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                theme::tag_ui(ui, state, *kind);
                                ui.add_space(6.0);
                                ui.label(lbl("→", 12.0, theme::FAINT));
                            });
                        })
                        .response;
                    let resp = inner.interact(Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(
                            resp.rect,
                            CornerRadius::same(7),
                            Color32::from_rgba_premultiplied(250, 250, 250, 8),
                        );
                    }
                    if resp.clicked() {
                        let from = self.session.route;
                        self.session.navigate(*route, self.session.scroll_of(from));
                    }
                    if i + 1 < stages.len() {
                        ui.add_space(4.0);
                    }
                }
            },
        );

        ui.add_space(10.0);
        ui.label(lbl(
            "当前为离线桌面演示。你可以通过右侧对话推进，也可以直接打开文档、代码与实验。所有演示结果均标注来源。",
            10.0,
            theme::FAINT,
        ));
    }

    // ---- 需求文档 ------------------------------------------------------------
    fn render_requirements(&mut self, ui: &mut egui::Ui) {
        let (req_saved, draft_dirty) = {
            let p = self.workspace.current();
            let saved = p.active_req.map(|id| p.reqs[id - 1].clone());
            let dirty = saved.as_ref().is_some_and(|r| {
                self.ai_req_text != r.text
                    || self.ai_req_acceptance != r.acceptance
                    || (self.ai_req_alloc / 100.0 - r.allocation).abs() > 1e-9
                    || self.ai_req_min_amount.trim().parse::<f64>().ok() != Some(r.min_amount)
            });
            (saved, dirty)
        };
        page_header(
            ui,
            "阶段 01 / 定义问题",
            "研究需求",
            "把想法转成明确的约束与可检验的标准。",
        );
        if draft_dirty {
            Self::warn_banner(
                ui,
                "需求草稿尚未确认或保存。运行使用已确认版本，草稿尚未生效。",
            );
        }

        panel(
            ui,
            "需求文档",
            Some(("可直接编辑", TagKind::Neutral)),
            |ui| {
                ui.label(field("研究目标与处理逻辑"));
                let mut text = self.ai_req_text.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut text)
                            .desired_rows(4)
                            .desired_width(ui.available_width()),
                    )
                    .changed()
                {
                    self.ai_req_text = text;
                }
                ui.add_space(12.0);
                ui.label(field("验收标准"));
                let mut acc = self.ai_req_acceptance.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut acc)
                            .desired_rows(3)
                            .desired_width(ui.available_width()),
                    )
                    .changed()
                {
                    self.ai_req_acceptance = acc;
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(field("目标投入比例（%）"));
                        ui.add(
                            egui::DragValue::new(&mut self.ai_req_alloc)
                                .range(0.0..=100.0)
                                .speed(1.0),
                        );
                    });
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.label(field("最低成交额（元）"));
                        let mut v = self.ai_req_min_amount.clone();
                        if ui
                            .add(egui::TextEdit::singleline(&mut v).desired_width(160.0))
                            .changed()
                        {
                            self.ai_req_min_amount = v;
                        }
                    });
                });
                ui.add_space(14.0);
                if ui.add(theme::primary_button("确认需求")).clicked() {
                    self.do_confirm_requirement();
                }
                ui.add_space(10.0);
                ui.label(note(
                "确认后生成需求版本。修改上游规格会使旧设计、代码与验证结论过期，历史实验仍保持原样。",
            ));
            },
        );

        ui.add_space(6.0);
        let history: Vec<(usize, String, bool)> = {
            let p = self.workspace.current();
            p.reqs
                .iter()
                .rev()
                .map(|r| {
                    (
                        r.id,
                        format!(
                            "需求 R{} · 投入 {:.0}% · 成交额 ≥{:.0}",
                            r.id,
                            r.allocation * 100.0,
                            r.min_amount
                        ),
                        p.active_req == Some(r.id),
                    )
                })
                .collect()
        };
        panel(ui, "确认历史", None, |ui| {
            if history.is_empty() {
                ui.label(note("还没有确认版本。草稿保留在当前项目内，重启会重置。"));
            } else {
                for (id, desc, active) in history {
                    ui.horizontal(|ui| {
                        ui.label(lbl(desc, 11.0, theme::TEXT2));
                        if active {
                            ui.add_space(6.0);
                            theme::tag_ui(ui, "当前", TagKind::Accent);
                        }
                        let _ = id;
                    });
                }
            }
        });
        let _ = req_saved;
    }

    // ---- 策略设计 ------------------------------------------------------------
    fn render_design(&mut self, ui: &mut egui::Ui) {
        let (req_ok, design_state) = {
            let p = self.workspace.current();
            let req_ok = p.active_req.is_some();
            let state = match p.active_design {
                Some(d) => {
                    let fresh = p
                        .active_req
                        .is_some_and(|r| p.designs.get(d - 1).is_some_and(|x| x.req_id == r));
                    format!("D{d} {}", if fresh { "已保存" } else { "已过期" })
                }
                None => "设计草稿".into(),
            };
            (req_ok, state)
        };
        let kind = if design_state.contains("已保存") {
            TagKind::Accent
        } else {
            TagKind::Neutral
        };
        page_header(
            ui,
            "阶段 02 / 处理逻辑",
            "策略设计",
            "文档、处理图与代码，共用同一份策略逻辑。",
        );
        if !req_ok {
            if empty_panel(
                ui,
                "等待确认需求",
                "系统流程图与时序图会从同一份策略结构生成。",
                Some("打开需求文档"),
            ) {
                let from = self.session.route;
                self.session
                    .navigate(Route::Requirements, self.session.scroll_of(from));
            }
            return;
        }

        panel(ui, "设计说明", Some((&design_state, kind)), |ui| {
            let mut note_text = self.ai_design_note.clone();
            if ui
                .add(
                    egui::TextEdit::multiline(&mut note_text)
                        .desired_rows(5)
                        .desired_width(ui.available_width()),
                )
                .changed()
            {
                self.ai_design_note = note_text;
            }
            ui.add_space(12.0);
            let (alloc, min_amt) = {
                let p = self.workspace.current();
                let r = &p.reqs[p.active_req.unwrap_or(1) - 1];
                (r.allocation, r.min_amount)
            };
            kv(
                ui,
                &[
                    ("投入比例", format!("{:.0}%（沿用需求版本）", alloc * 100.0)),
                    ("最低成交额", format!("≥{min_amt:.0} 元")),
                ],
            );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(theme::primary_button("保存设计版本")).clicked() {
                    self.do_generate_design();
                }
                ui.add_space(8.0);
                if ui.add(theme::ghost_button("从需求生成设计")).clicked() {
                    let req_text = self.workspace.current().reqs
                        [self.workspace.current().active_req.unwrap_or(1) - 1]
                        .text
                        .clone();
                    self.ai_design_note = format!(
                        "按需求 R：{req_text}\n\n处理顺序：可见数据 → 股票池过滤 → 信号形成 → 仓位计算 → 模拟成交 → 指标计算。信号日与执行日分离。"
                    );
                }
            });
        });

        ui.add_space(6.0);
        panel(ui, "策略处理图", None, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.vertical(|ui| {
                    ui.label(note("图描述预期行为；运行事件描述实际结果。"));
                });
                if ui
                    .add(tab_button("时序图", !self.design_tab_flow))
                    .clicked()
                {
                    self.design_tab_flow = false;
                }
                ui.add_space(4.0);
                if ui.add(tab_button("流程图", self.design_tab_flow)).clicked() {
                    self.design_tab_flow = true;
                }
            });
            ui.add_space(10.0);
            if self.design_tab_flow {
                // 流程图：2×3 节点卡（静态结构）
                for row in 0..2 {
                    ui.horizontal(|ui| {
                        for col in 0..3 {
                            let idx = row * 3 + col;
                            self.node_card(ui, idx);
                            if col < 2 {
                                ui.add_space(4.0);
                                ui.centered_and_justified(|ui| {
                                    ui.label(lbl("→", 12.0, theme::ACCENT));
                                });
                                ui.add_space(4.0);
                            }
                        }
                    });
                    ui.add_space(6.0);
                }
                ui.label(note(
                    "跨交易日 · 信号与执行分离（信号日收盘形成信号，下一交易日开盘执行）。",
                ));
            } else {
                // 时序图：文字化时序（六节点五步）
                kv(
                    ui,
                    &[
                        (
                            "信号日",
                            "① 可见行情 → ② 入选样本 → ③ 目标比例（收盘后形成信号）".to_string(),
                        ),
                        (
                            "执行日",
                            "④ 资金约束 → ⑤ 成交回报（此时才读取开盘价）".to_string(),
                        ),
                        ("估值", "现金与持仓按当日收盘估值".to_string()),
                    ],
                );
            }
            ui.add_space(10.0);
            // 节点检查
            let (name, io, line) = {
                let n = &NODES[self.design_node.min(NODES.len() - 1)];
                (n.1, n.3, n.4)
            };
            ui.separator();
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(lbl(format!("节点检查 · {name}"), 12.0, theme::TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    theme::tag_ui(
                        ui,
                        NODES[self.design_node.min(NODES.len() - 1)].0,
                        TagKind::Neutral,
                    );
                });
            });
            ui.add_space(6.0);
            kv(
                ui,
                &[
                    ("输入 / 输出", io.to_string()),
                    ("源码映射", format!("示例源码第 {line} 行")),
                ],
            );
        });
    }

    /// 流程图节点卡（选中态绿描边）。
    fn node_card(&mut self, ui: &mut egui::Ui, idx: usize) {
        let idx = idx.min(NODES.len() - 1);
        let (id, name, sub, _io, _line) = NODES[idx];
        let selected = self.design_node.min(NODES.len() - 1) == idx;
        let inner = Frame::NONE
            .fill(Color32::from_rgb(0x14, 0x18, 0x16))
            .corner_radius(CornerRadius::same(9))
            .stroke(Stroke::new(
                1.0,
                if selected {
                    theme::ACCENT
                } else {
                    Color32::from_rgb(0x24, 0x3B, 0x2E)
                },
            ))
            .inner_margin(Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_min_width((ui.available_width() - 40.0) / 3.0);
                ui.label(lbl(format!("{:02}　{name}", idx + 1), 11.0, theme::TEXT));
                ui.label(lbl(sub, 9.0, Color32::from_rgb(0x6B, 0x7F, 0x74)));
                let _ = id;
            })
            .response;
        let resp = inner.interact(Sense::click());
        if resp.clicked() {
            self.design_node = idx;
        }
    }

    // ---- 策略开发 ------------------------------------------------------------
    fn render_develop(&mut self, ui: &mut egui::Ui) {
        let (design_ok, version_state) = {
            let p = self.workspace.current();
            let design_ok = p
                .active_req
                .zip(p.active_design)
                .is_some_and(|(r, d)| p.designs.get(d - 1).is_some_and(|x| x.req_id == r));
            let state = match p.active_version {
                Some(v) => {
                    let fresh = self.workspace.version_fresh(v);
                    format!("v{v} {}", if fresh { "当前" } else { "过期" })
                }
                None => "未保存".into(),
            };
            (design_ok, state)
        };
        page_header(
            ui,
            "阶段 03 / 策略实现",
            "策略开发",
            "保存明确的版本，再把它交给实验与验证。",
        );
        if !design_ok {
            Self::warn_banner(ui, "当前没有匹配需求的设计。请先在策略设计页保存设计版本。");
        }

        panel(
            ui,
            "策略源码",
            Some((&version_state, TagKind::Accent)),
            |ui| {
                let mut src = self.ai_code_source.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut src)
                            .desired_rows(10)
                            .desired_width(ui.available_width())
                            .font(FontId::monospace(12.0)),
                    )
                    .changed()
                {
                    self.ai_code_source = src;
                }
                ui.add_space(8.0);
                ui.label(note("源码为研究草稿记录，桌面不在本地执行任意代码；实验运行由协调器以 EMA 模板与运行参数执行。"));
                ui.add_space(12.0);
                if ui
                    .add(theme::primary_button("保存版本（不可变）"))
                    .clicked()
                {
                    self.do_save_version();
                }
            },
        );

        ui.add_space(6.0);
        let versions: Vec<(usize, usize, usize, bool, bool)> = {
            let p = self.workspace.current();
            p.versions
                .iter()
                .rev()
                .map(|v| {
                    (
                        v.id,
                        v.req_id,
                        v.design_id,
                        self.workspace.version_fresh(v.id),
                        p.active_version == Some(v.id),
                    )
                })
                .collect()
        };
        panel(ui, "不可变版本", None, |ui| {
            if versions.is_empty() {
                ui.label(note("检查草稿并保存后，版本会出现在这里。保存后不可覆盖。"));
            }
            for (id, req, design, fresh, active) in versions {
                ui.horizontal(|ui| {
                    ui.label(mono(
                        format!("v{id} · R{req} / D{design}"),
                        11.0,
                        theme::TEXT2,
                    ));
                    ui.add_space(8.0);
                    theme::tag_ui(
                        ui,
                        if active {
                            "当前"
                        } else if fresh {
                            "可选"
                        } else {
                            "过期"
                        },
                        if active {
                            TagKind::Accent
                        } else {
                            TagKind::Neutral
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(theme::ghost_button("查看")).clicked() {
                            self.view_source = self
                                .workspace
                                .current()
                                .versions
                                .get(id - 1)
                                .map(|v| (v.id, v.source.clone()));
                        }
                    });
                });
                ui.add_space(6.0);
            }
        });
    }

    // ---- 事件调试 ------------------------------------------------------------
    fn render_debug(&mut self, ui: &mut egui::Ui) {
        let exps: Vec<(usize, usize, Option<String>, Option<String>)> = self
            .workspace
            .current()
            .experiments
            .iter()
            .map(|e| {
                (
                    e.id,
                    e.version_id,
                    e.task_id.clone(),
                    e.total_return.clone(),
                )
            })
            .collect();
        page_header(
            ui,
            "阶段 04 / 解释运行行为",
            "事件调试",
            "回放实验证据，查看任务状态与比较归因。",
        );
        if exps.is_empty() {
            if empty_panel(
                ui,
                "还没有实验记录",
                "先保存一个策略版本并运行实验。历史实验与任务证据在此保留。",
                Some("打开策略开发"),
            ) {
                let from = self.session.route;
                self.session
                    .navigate(Route::Develop, self.session.scroll_of(from));
            }
            return;
        }

        // 当前版本状态 + 运行/取消
        let (vid, fresh) = {
            let p = self.workspace.current();
            (
                p.active_version,
                p.active_version
                    .map(|v| self.workspace.version_fresh(v))
                    .unwrap_or(false),
            )
        };
        panel(
            ui,
            "当前版本与运行",
            Some(("证据来自任务引用", TagKind::Neutral)),
            |ui| {
                match vid {
                    Some(v) => ui.label(lbl(
                        format!(
                            "当前版本 v{v}：{}",
                            if fresh {
                                "可运行"
                            } else {
                                "已过期，请重新保存版本"
                            }
                        ),
                        12.0,
                        if fresh { theme::ACCENT } else { theme::RED },
                    )),
                    None => ui.label(lbl("尚无版本：请先在策略开发页保存", 12.0, theme::RED)),
                };
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("运行实验")).clicked() {
                        self.do_ai_run();
                    }
                    if self.ai_run.watch.is_active() {
                        if let Some(t) = self.ai_run.watch.task_id() {
                            let t = t.to_string();
                            if ui.add(theme::ghost_button("取消")).clicked() {
                                self.do_cancel(t, PageKey::Run);
                            }
                        }
                    }
                });
                render_watch(ui, &self.ai_run);
                ui.add_space(8.0);
                ui.label(note("调试解释代码为什么这样运行；回测回答历史模拟表现。此处为任务级证据，逐事件回放属于后续生产能力。"));
            },
        );

        ui.add_space(6.0);
        panel(
            ui,
            "实验记录",
            Some(("历史实验不变", TagKind::Neutral)),
            |ui| {
                egui::Grid::new("debug-exp-grid").show(ui, |ui| {
                    for head in ["实验", "版本", "任务", "期末收益"] {
                        ui.label(mono(head, 10.0, theme::FAINT));
                    }
                    ui.end_row();
                    for (id, ver, task, ret) in &exps {
                        ui.label(mono(format!("E{id}"), 11.0, theme::TEXT2));
                        ui.label(mono(format!("v{ver}"), 11.0, theme::TEXT2));
                        ui.label(mono(
                            task.clone().unwrap_or_else(|| "（无任务）".into()),
                            11.0,
                            theme::MUTED,
                        ));
                        ui.label(mono(
                            ret.clone().unwrap_or_else(|| "—".into()),
                            11.0,
                            theme::TEXT2,
                        ));
                        ui.end_row();
                    }
                });
            },
        );

        ui.add_space(6.0);
        let ids: Vec<usize> = exps.iter().map(|(id, _, _, _)| *id).collect();
        let cmp_a = self.debug_cmp[0];
        let cmp_b = self.debug_cmp[1];
        panel(ui, "实验比较归因", None, |ui| {
            ui.horizontal(|ui| {
                ui.label(field("实验 A"));
                egui::ComboBox::from_id_salt("debug-cmp-a")
                    .selected_text(format!("E{cmp_a}"))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for id in &ids {
                            ui.selectable_value(&mut self.debug_cmp[0], *id, format!("E{id}"));
                        }
                    });
                ui.add_space(10.0);
                ui.label(field("实验 B"));
                egui::ComboBox::from_id_salt("debug-cmp-b")
                    .selected_text(format!("E{cmp_b}"))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        for id in &ids {
                            ui.selectable_value(&mut self.debug_cmp[1], *id, format!("E{id}"));
                        }
                    });
                ui.add_space(10.0);
                if ui.add(theme::ghost_button("比较")).clicked() {
                    let (a, b) = (self.debug_cmp[0], self.debug_cmp[1]);
                    match self.workspace.compare_experiments(a, b) {
                        Ok((same, diffs)) => {
                            let text = if same {
                                format!(
                                    "E{a} 与 E{b}：同输入（同版本同修订戳）。若结果分歧，归因代码。"
                                )
                            } else {
                                format!(
                                    "E{a} 与 E{b}：输入不同：{}。仅并列查看，不作代码效果归因。",
                                    diffs.join("；")
                                )
                            };
                            self.tell(text, None);
                        }
                        Err(e) => self.ai_notice = Some(e),
                    }
                }
            });
            ui.add_space(8.0);
            ui.label(note(
                "同输入实验的分歧归因代码；跨输入实验先列出输入差异，不归因代码。",
            ));
        });
        self.poll(PageKey::Run);
    }

    // ---- 回测实验 ------------------------------------------------------------
    fn render_experiments(&mut self, ui: &mut egui::Ui) {
        let (versions_len, exps_len, last_ret, exps) = {
            let p = self.workspace.current();
            (
                p.versions.len(),
                p.experiments.len(),
                p.experiments
                    .iter()
                    .rev()
                    .find_map(|e| e.total_return.clone()),
                p.experiments
                    .iter()
                    .map(|e| {
                        (
                            e.id,
                            e.version_id,
                            e.task_id.clone(),
                            e.total_return.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        };
        page_header(
            ui,
            "实验 / 历史模拟",
            "回测实验",
            "冻结输入、运行模拟，再比较结果。",
        );

        // 指标卡 ×3
        ui.horizontal(|ui| {
            let w = (ui.available_width() - 24.0) / 3.0;
            for (top, value, bottom) in [
                (
                    "策略版本",
                    format!("{versions_len:02}"),
                    "保存后不可覆盖".to_string(),
                ),
                (
                    "已完成实验",
                    format!("{exps_len:02}"),
                    "输入与结果可追溯".to_string(),
                ),
                (
                    "最近收益",
                    last_ret.clone().unwrap_or_else(|| "—".into()),
                    "完成实验后回填".to_string(),
                ),
            ] {
                ui.allocate_ui(sz(w, 0.0), |ui| {
                    Frame::NONE
                        .fill(Color32::from_rgba_premultiplied(250, 250, 250, 8))
                        .corner_radius(CornerRadius::same(10))
                        .stroke(Stroke::new(1.0, theme::BORDER))
                        .inner_margin(Margin::same(17))
                        .show(ui, |ui| {
                            ui.label(lbl(top, 10.0, theme::MUTED));
                            ui.add_space(8.0);
                            ui.label(mono(value, 23.0, theme::TEXT));
                            ui.add_space(4.0);
                            ui.label(lbl(bottom, 10.0, theme::MUTED));
                        });
                });
                ui.add_space(12.0);
            }
        });
        ui.add_space(4.0);

        // 运行参数 + 提交（真实协调器执行，冻结当前版本）
        let snapshot_status = self
            .pick_snapshot()
            .map(|(_, id)| id)
            .unwrap_or_else(|| "尚无（请在数据中心导入）".into());
        let universe_status = self
            .universe_saved
            .as_ref()
            .map(|u| u.universe_id.clone())
            .unwrap_or_else(|| "尚无（请在股票池保存）".into());
        panel(
            ui,
            "运行参数",
            Some(("EMA 模板 · 严格模式", TagKind::Neutral)),
            |ui| {
                ui.horizontal(|ui| {
                    for (name, key) in [("fast", 0u8), ("slow", 1), ("top_k", 2)] {
                        ui.label(mono(name, 11.0, theme::MUTED));
                        match key {
                            0 => {
                                ui.add(egui::DragValue::new(&mut self.run_form.fast));
                            }
                            1 => {
                                ui.add(egui::DragValue::new(&mut self.run_form.slow));
                            }
                            _ => {
                                ui.add(egui::DragValue::new(&mut self.run_form.top_k));
                            }
                        }
                        ui.add_space(10.0);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(field("起止"));
                    let mut s = self.run_form.start.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut s).desired_width(100.0))
                        .changed()
                    {
                        self.run_form.start = s;
                    }
                    let mut e = self.run_form.end.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut e).desired_width(100.0))
                        .changed()
                    {
                        self.run_form.end = e;
                    }
                    ui.add_space(10.0);
                    ui.label(field("资金"));
                    let mut c = self.run_form.capital.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut c).desired_width(100.0))
                        .changed()
                    {
                        self.run_form.capital = c;
                    }
                });
                ui.collapsing("成本假设", |ui| {
                    for (name, field_i) in [
                        ("佣金率", 0u8),
                        ("最低佣金", 1),
                        ("卖出税", 2),
                        ("其他费", 3),
                        ("滑点 bps", 4),
                        ("参与率", 5),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(field(name));
                            let mut v = match field_i {
                                0 => self.run_form.commission_rate.clone(),
                                1 => self.run_form.min_commission.clone(),
                                2 => self.run_form.sell_tax.clone(),
                                3 => self.run_form.other_fee.clone(),
                                4 => self.run_form.slippage_bps.clone(),
                                _ => self.run_form.participation.clone(),
                            };
                            if ui
                                .add(egui::TextEdit::singleline(&mut v).desired_width(120.0))
                                .changed()
                            {
                                match field_i {
                                    0 => self.run_form.commission_rate = v,
                                    1 => self.run_form.min_commission = v,
                                    2 => self.run_form.sell_tax = v,
                                    3 => self.run_form.other_fee = v,
                                    4 => self.run_form.slippage_bps = v,
                                    _ => self.run_form.participation = v,
                                }
                            }
                        });
                    }
                });
                kv(
                    ui,
                    &[("数据快照", snapshot_status), ("股票池", universe_status)],
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("运行当前版本")).clicked() {
                        self.do_ai_run();
                    }
                    if self.ai_run.watch.is_active() {
                        if let Some(t) = self.ai_run.watch.task_id() {
                            let t = t.to_string();
                            if ui.add(theme::ghost_button("取消")).clicked() {
                                self.do_cancel(t, PageKey::Run);
                            }
                        }
                    }
                });
                render_watch(ui, &self.ai_run);
            },
        );

        ui.add_space(6.0);
        panel(
            ui,
            "实验记录",
            Some(("证据可追溯", TagKind::Neutral)),
            |ui| {
                if exps.is_empty() {
                    ui.label(note("还没有实验。选择已保存且未过期的策略版本运行。"));
                } else {
                    egui::Grid::new("exp-grid").show(ui, |ui| {
                        for head in ["实验", "版本", "任务", "期末收益", ""] {
                            ui.label(mono(head, 10.0, theme::FAINT));
                        }
                        ui.end_row();
                        for (id, ver, task, ret) in &exps {
                            ui.label(mono(format!("E{id} / v{ver}"), 11.0, theme::TEXT2));
                            ui.label(mono(
                                task.clone().unwrap_or_else(|| "—".into()),
                                11.0,
                                theme::MUTED,
                            ));
                            ui.label(mono(
                                ret.clone().unwrap_or_else(|| "—".into()),
                                11.0,
                                theme::TEXT2,
                            ));
                            ui.end_row();
                        }
                    });
                }
            },
        );

        ui.add_space(6.0);
        // 运行比较（协调器运行 ID 比较）
        panel(ui, "运行比较", None, |ui| {
            ui.label(note(
                "2..5 个运行 ID（逗号分割；ID 见任务记录），跨输入比较不归因代码。",
            ));
            let mut text = self.compare_form.run_ids_text.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(ui.available_width())
                        .font(FontId::monospace(12.0)),
                )
                .changed()
            {
                self.compare_form.run_ids_text = text;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.compare_form.intersection, "交集视图");
                if ui.add(theme::ghost_button("比较")).clicked() {
                    self.do_compare();
                }
            });
            if let Some(err) = &self.compare_error {
                ui.label(lbl(err, 11.0, theme::RED));
            }
            if let Some(cmp) = &self.compare_result {
                ui.add_space(8.0);
                egui::Grid::new("compare-grid").show(ui, |ui| {
                    for head in ["运行", "区间", "总收益", "夏普", "回撤"] {
                        ui.label(mono(head, 10.0, theme::FAINT));
                    }
                    ui.end_row();
                    for r in &cmp.runs {
                        let m = &r.metrics;
                        ui.label(mono(
                            &r.run_id[..12.min(r.run_id.len())],
                            11.0,
                            theme::TEXT2,
                        ));
                        ui.label(mono(format!("{}~{}", r.start, r.end), 11.0, theme::TEXT2));
                        ui.label(mono(fmt_metric(&m.total_return), 11.0, theme::TEXT2));
                        ui.label(mono(fmt_metric(&m.sharpe), 11.0, theme::TEXT2));
                        ui.label(mono(fmt_metric(&m.max_drawdown), 11.0, theme::TEXT2));
                        ui.end_row();
                    }
                });
                for d in &cmp.differences {
                    ui.label(lbl(
                        format!("差异 [{}] {}", d.kind, d.detail),
                        11.0,
                        theme::ACCENT_CYAN,
                    ));
                }
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_button("生成当前版本验证报告"))
                .clicked()
            {
                self.do_make_report();
            }
            ui.add_space(8.0);
            theme::tag_ui(ui, "回测不是策略验证", TagKind::Warn);
        });
        self.poll(PageKey::Run);
    }

    // ---- 验证报告 ------------------------------------------------------------
    fn render_validate(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "阶段 05 / 检查证据",
            "策略验证",
            "先看证据，再形成有边界的结论。",
        );
        Self::warn_banner(
            ui,
            "演示检查通过，只表示本样本下的流程与约束检查完成。真实策略验证仍需真实数据、样本外和稳健性证据。",
        );

        let report = self.workspace.current().report.clone();
        match report {
            Some(rep) => {
                let rd = {
                    let p = self.workspace.current();
                    p.versions
                        .get(rep.version_id - 1)
                        .map(|v| (v.req_id, v.design_id))
                };
                let head_badge = rd
                    .map(|(r, d)| format!("R{r} / D{d}"))
                    .unwrap_or_else(|| "—".into());
                panel(
                    ui,
                    &format!("当前版本 · v{}", rep.version_id),
                    Some((&head_badge, TagKind::Neutral)),
                    |ui| {
                        ui.horizontal(|ui| {
                            theme::tag_ui(
                                ui,
                                &format!(
                                    "演示检查：{}",
                                    if rep.demo_pass() { "通过" } else { "失败" }
                                ),
                                if rep.demo_pass() {
                                    TagKind::Accent
                                } else {
                                    TagKind::Bad
                                },
                            );
                            theme::tag_ui(ui, "正式验证：证据不足", TagKind::Warn);
                        });
                        ui.add_space(10.0);
                        for (name, ok, detail) in &rep.checks {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(lbl(name, 11.0, theme::TEXT2));
                                    ui.label(lbl(detail, 10.0, theme::ACCENT_TEXT));
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    theme::tag_ui(
                                        ui,
                                        if *ok { "通过" } else { "失败" },
                                        if *ok { TagKind::Accent } else { TagKind::Bad },
                                    );
                                });
                            });
                            ui.add_space(8.0);
                        }
                        ui.add_space(4.0);
                        if ui
                            .add(theme::ghost_button(&format!(
                                "打开实验 E{} 的证据 →",
                                rep.run_id
                            )))
                            .clicked()
                        {
                            let from = self.session.route;
                            self.session
                                .navigate(Route::Debug, self.session.scroll_of(from));
                        }
                    },
                );
            }
            None => {
                if empty_panel(
                    ui,
                    "尚未形成验证结论",
                    "先运行当前代码版本。报告只使用实际产生的实验，不用手工勾选替代证据。",
                    Some("打开回测实验"),
                ) {
                    let from = self.session.route;
                    self.session
                        .navigate(Route::Experiments, self.session.scroll_of(from));
                }
            }
        }

        ui.add_space(6.0);
        panel(ui, "正式策略验证", None, |ui| {
            for (name, state) in [
                ("真实行情与历史可见信息检验", "未运行"),
                ("独立样本外区间评估", "未运行"),
                ("参数与成本敏感性检验", "未运行"),
                ("市场适用范围与失效条件评审", "证据不足"),
            ] {
                ui.horizontal(|ui| {
                    ui.label(lbl(name, 11.0, theme::TEXT2));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        theme::tag_ui(ui, state, TagKind::Warn);
                    });
                });
                ui.add_space(8.0);
            }
        });

        ui.add_space(6.0);
        panel(ui, "四个概念，四种职责", None, |ui| {
            ui.label(concepts(
                "调试：解释代码为什么这样运行。\n回测：观察历史模拟结果。\n策略验证：综合证据判断是否满足研究标准。\n计划核对：检查这一次调整是否满足当前账户与数据约束。",
            ));
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.add(theme::primary_button("继续演示交易计划")).clicked() {
                let from = self.session.route;
                self.session
                    .navigate(Route::Plan, self.session.scroll_of(from));
            }
            ui.add_space(8.0);
            ui.label(note("仅演示核对流程，不构成正式交易许可"));
        });
        ui.add_space(8.0);
        if ui.add(theme::primary_button("生成验证报告")).clicked() {
            self.do_make_report();
        }
    }

    // ---- 交易计划 ------------------------------------------------------------
    fn render_plan_page(&mut self, ui: &mut egui::Ui) {
        let ready = report_ready(&self.workspace);
        page_header(
            ui,
            "阶段 06 / 从策略到计划",
            "交易计划",
            "一次前向核对：现在持有什么，下一步计划调整什么。",
        );
        if !ready {
            Self::warn_banner(
                ui,
                "当前没有可用于计划演示的验证版本。请先完成当前版本的演示检查；正式验证仍为证据不足。",
            );
        }

        // 计划桥（S20：冻结签名 + 核对门控 + 确认导出）
        let plan_source = {
            let p = self.workspace.current();
            p.plan
                .as_ref()
                .map(|t| format!("v{} · {} · {}", t.version_id, t.snapshot, t.trade_date))
        };
        panel(
            ui,
            "计划输入",
            Some(("独立账户快照", TagKind::Neutral)),
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(field("交易日"));
                    let mut d = self.ai_plan_trade_date.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut d).desired_width(120.0))
                        .changed()
                    {
                        self.ai_plan_trade_date = d;
                    }
                    ui.add_space(10.0);
                    ui.label(note("数据快照取最新已导入"));
                });
                ui.add_space(8.0);
                ui.label(field("手工持仓 JSON"));
                let mut json = self.ai_plan_json.clone();
                if ui
                .add(
                    egui::TextEdit::multiline(&mut json)
                        .desired_rows(4)
                        .desired_width(ui.available_width())
                        .font(FontId::monospace(12.0))
                        .hint_text(r#"{"cash_cny":"10000","total_assets_cny":"20000","positions":[…]}"#),
                )
                .changed()
            {
                self.ai_plan_json = json;
            }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("生成计划")).clicked() {
                        self.do_ai_plan_generate();
                    }
                    ui.add_space(8.0);
                    if ui.add(theme::ghost_button("核对")).clicked() {
                        self.do_ai_plan_check();
                    }
                    ui.add_space(8.0);
                    if ui.add(theme::primary_button("确认导出")).clicked() {
                        self.do_ai_plan_export();
                    }
                });
                if let Some(issues) = &self.ai_plan_issues {
                    ui.add_space(8.0);
                    if issues.is_empty() {
                        ui.label(lbl("核对通过：可确认导出", 11.0, theme::ACCENT_TEXT));
                    } else {
                        for i in issues {
                            ui.label(lbl(format!("⚠ {i}"), 11.0, theme::RED));
                        }
                    }
                }
                if let Some(src) = &plan_source {
                    ui.add_space(8.0);
                    kv(ui, &[("来源", src.clone())]);
                }
                if let Some(csv) = &self.ai_plan_csv {
                    ui.add_space(8.0);
                    ui.label(note("导出内容（含演示标识）："));
                    ui.label(mono(csv, 11.0, theme::TEXT));
                }
                ui.add_space(6.0);
                ui.label(note(
                    "计划使用独立参考快照，绑定策略版本并冻结账户输入；确认导出不发送任何订单。",
                ));
            },
        );

        ui.add_space(6.0);
        // 协调器计划（生产对接面板：生成 → 导出 → 备注）
        panel(ui, "协调器计划（生产对接）", None, |ui| {
            ui.horizontal(|ui| {
                ui.label(field("as_of"));
                let mut as_of = self.plan_form.as_of.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut as_of).desired_width(120.0))
                    .changed()
                {
                    self.plan_form.as_of = as_of;
                }
            });
            ui.add_space(8.0);
            ui.label(field("手工持仓 JSON（空 = 使用最近持有版本）"));
            let mut holdings = self.plan_form.holdings_json.clone();
            if ui
                .add(
                    egui::TextEdit::multiline(&mut holdings)
                        .desired_rows(3)
                        .desired_width(ui.available_width())
                        .font(FontId::monospace(12.0)),
                )
                .changed()
            {
                self.plan_form.holdings_json = holdings;
            }
            ui.add_space(10.0);
            let enabled = self.bridge.is_some() && !self.plan_page.watch.is_active();
            ui.add_enabled_ui(enabled, |ui| {
                if ui
                    .add(theme::primary_button("生成计划（协调器）"))
                    .clicked()
                {
                    self.do_plan();
                }
            });
            render_watch(ui, &self.plan_page);
            if let Some(v) = self.plan_page.watch.terminal() {
                if crate::pipeline::is_success(v) {
                    if let Some(plan_id) = self.plan_page.watch.task_id() {
                        let plan_id = plan_id.to_string();
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let mut path = self.plan_form.export_path.clone();
                            let resp =
                                ui.add(egui::TextEdit::singleline(&mut path).desired_width(300.0));
                            if resp.changed() {
                                self.plan_form.export_path = path.clone();
                            }
                            if ui.add(theme::ghost_button("导出 CSV")).clicked() && !path.is_empty()
                            {
                                self.do_export(&plan_id);
                            }
                        });
                        ui.horizontal(|ui| {
                            let mut note_text = self.plan_form.note_text.clone();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut note_text).desired_width(300.0),
                                )
                                .changed()
                            {
                                self.plan_form.note_text = note_text;
                            }
                            if ui.add(theme::ghost_button("保存备注")).clicked()
                                && !self.plan_form.note_text.is_empty()
                            {
                                self.do_note(&plan_id);
                            }
                        });
                    }
                }
            }
            if let Some(info) = &self.plan_exported {
                ui.add_space(6.0);
                ui.label(lbl(info, 11.0, theme::ACCENT_TEXT));
            }
        });

        ui.add_space(6.0);
        panel(ui, "计划核对不是回测", None, |ui| {
            ui.label(concepts(
                "回测在历史区间反复模拟策略；计划核对对一个交易日的具体调整做约束核对。核对通过不能保证成交或盈利。文件始终标注“演示计划”，不产生订单。",
            ));
        });
        self.poll(PageKey::Plan);
    }

    // ---- 数据中心 ------------------------------------------------------------
    fn render_data(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "资源 / 行情与快照",
            "数据中心",
            "统一管理研究输入，历史实验始终引用原快照。",
        );
        panel(
            ui,
            "数据导入",
            Some(("可重复导入", TagKind::Neutral)),
            |ui| {
                ui.label(field("导入暂存数据（每行一个 CSV 路径）"));
                let mut edited = self.import_form.paths.join("\n");
                let response = ui.add(
                    egui::TextEdit::multiline(&mut edited)
                        .desired_rows(3)
                        .desired_width(ui.available_width()),
                );
                if response.changed() {
                    self.import_form.paths = edited
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_string)
                        .collect();
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(field("价格口径"));
                    ui.add_space(4.0);
                    for (i, name) in ["raw", "qfq", "hfq"].iter().enumerate() {
                        if ui.radio(self.import_form.price_basis == i, *name).clicked() {
                            self.import_form.price_basis = i;
                        }
                    }
                });
                ui.add_space(10.0);
                let enabled = self.bridge.is_some() && !self.snapshot_page.watch.is_active();
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.add(theme::primary_button("导入")).clicked() {
                        self.do_import();
                    }
                });
                render_watch(ui, &self.snapshot_page);
            },
        );

        ui.add_space(6.0);
        let snapshots: Vec<(String, String, String)> = self
            .bridge
            .as_ref()
            .and_then(|b| b.snapshots(20).ok())
            .map(|p| {
                p.items
                    .iter()
                    .map(|s| {
                        (
                            s.snapshot_id.clone(),
                            s.as_of.clone(),
                            s.manifest_hash.chars().take(12).collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        panel(ui, "快照清单", None, |ui| {
            if snapshots.is_empty() {
                ui.label(note(
                    self.bridge_error
                        .as_deref()
                        .unwrap_or("尚无快照。导入后历史实验将引用原快照。"),
                ));
            }
            for (id, as_of, hash) in snapshots {
                ui.label(mono(
                    format!("{id}  as_of={as_of}  清单 {hash}…"),
                    11.0,
                    theme::TEXT,
                ));
                ui.add_space(4.0);
            }
            ui.add_space(4.0);
            ui.label(note(
                "模拟更新或再次导入会创建新的快照引用，使当前版本与结论过期；历史实验不变。",
            ));
        });
        self.poll(PageKey::Snapshot);
    }

    // ---- 股票池 --------------------------------------------------------------
    fn render_pool(&mut self, ui: &mut egui::Ui) {
        let saved_state = self
            .universe_saved
            .as_ref()
            .map(|u| format!("已保存 {}", u.universe_id));
        page_header(
            ui,
            "资源 / 可研究的标的",
            "股票池",
            "规则预览与版本保存；历史实验继续引用当时的规则和成员。",
        );
        panel(
            ui,
            "筛选规则",
            Some((
                saved_state.as_deref().unwrap_or("未保存"),
                if self.universe_saved.is_some() {
                    TagKind::Accent
                } else {
                    TagKind::Neutral
                },
            )),
            |ui| {
                ui.label(field("规则 JSON（RuleAST）"));
                let mut rule = self.universe_form.rule_text.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut rule)
                            .desired_rows(4)
                            .desired_width(ui.available_width())
                            .font(FontId::monospace(12.0)),
                    )
                    .changed()
                {
                    self.universe_form.rule_text = rule;
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(field("as_of"));
                    let mut as_of = self.universe_form.as_of.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut as_of).desired_width(110.0))
                        .changed()
                    {
                        self.universe_form.as_of = as_of;
                    }
                    ui.add_space(10.0);
                    ui.checkbox(&mut self.universe_form.strict, "严格模式");
                    ui.checkbox(&mut self.universe_form.fixed_membership, "固定成员");
                });
                ui.add_space(10.0);
                let enabled = self.bridge.is_some() && !self.universe_page.watch.is_active();
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.add(theme::primary_button("预览")).clicked() {
                        self.do_preview();
                    }
                });
                render_watch(ui, &self.universe_page);

                // 预览成功：三态原因 + 保存版本
                if let Some(v) = self.universe_page.watch.terminal() {
                    if crate::pipeline::is_success(v) {
                        if let Some(bridge) = &self.bridge {
                            let task_id = self.universe_page.watch.task_id().map(str::to_string);
                            if let Some(task_id) = task_id {
                                if let Ok(p) = bridge.preview(&task_id) {
                                    ui.add_space(8.0);
                                    ui.label(lbl(
                                        format!(
                                            "通过 {} / 排除 {} / 未知 {}",
                                            p.pass, p.exclude, p.unknown
                                        ),
                                        12.0,
                                        theme::TEXT,
                                    ));
                                    ui.add_space(8.0);
                                    if ui.add(theme::primary_button("保存股票池版本")).clicked()
                                    {
                                        let (ph, ih) =
                                            (p.preview_hash.clone(), p.input_hash.clone());
                                        self.do_save_universe(&task_id, &ph, &ih);
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(u) = &self.universe_saved {
                    ui.add_space(6.0);
                    ui.label(lbl(
                        format!("已保存：{}（成员 {}）", u.universe_id, u.count),
                        11.0,
                        theme::ACCENT_TEXT,
                    ));
                }
            },
        );
        self.poll(PageKey::Universe);
    }

    // ---- 策略资产 ------------------------------------------------------------
    fn render_library(&mut self, ui: &mut egui::Ui) {
        let versions: Vec<(usize, usize, usize, bool, bool)> = {
            let p = self.workspace.current();
            p.versions
                .iter()
                .map(|v| {
                    (
                        v.id,
                        v.req_id,
                        v.design_id,
                        self.workspace.version_fresh(v.id),
                        p.active_version == Some(v.id),
                    )
                })
                .collect()
        };
        page_header(
            ui,
            "资源 / 可复用的策略",
            "策略资产",
            "当前项目的策略版本与上游引用。",
        );
        if versions.is_empty() {
            if empty_panel(
                ui,
                "尚无策略版本",
                "从需求与设计开始，代码保存后会归档到当前项目资产中。",
                Some("打开策略开发"),
            ) {
                let from = self.session.route;
                self.session
                    .navigate(Route::Develop, self.session.scroll_of(from));
            }
            return;
        }
        panel(
            ui,
            "版本库",
            Some(("当前项目", TagKind::Neutral)),
            |ui| {
                for (id, req, design, fresh, active) in versions {
                    ui.horizontal(|ui| {
                        ui.label(mono(format!("v{id}"), 12.0, theme::TEXT));
                        ui.add_space(6.0);
                        ui.label(mono(format!("R{req} / D{design}"), 11.0, theme::MUTED));
                        ui.add_space(8.0);
                        theme::tag_ui(
                            ui,
                            if active {
                                "当前选择"
                            } else if fresh {
                                "可用于研究"
                            } else {
                                "上游已过期"
                            },
                            if active {
                                TagKind::Accent
                            } else if fresh {
                                TagKind::Neutral
                            } else {
                                TagKind::Warn
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add(theme::ghost_button("查看")).clicked() {
                                self.view_source = self
                                    .workspace
                                    .current()
                                    .versions
                                    .get(id - 1)
                                    .map(|v| (v.id, v.source.clone()));
                            }
                        });
                    });
                    ui.add_space(6.0);
                }
                ui.add_space(4.0);
                if ui.add(theme::primary_button("打开编辑器")).clicked() {
                    let from = self.session.route;
                    self.session
                        .navigate(Route::Develop, self.session.scroll_of(from));
                }
            },
        );
    }

    // ---- 浮层 ----------------------------------------------------------------
    fn render_windows(&mut self, ctx: &egui::Context) {
        // 运行记录（任务动作、依据和产物；不展示模型内部推理）
        egui::Window::new("运行记录与任务控制")
            .open(&mut self.show_logs)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(note(
                    "只记录任务动作、依据和产物，不展示模型内部推理。所有任务均为离线演示。",
                ));
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        if self.logs.is_empty() {
                            ui.label(note("暂无任务记录"));
                        }
                        for l in &self.logs {
                            ui.label(mono(l, 11.0, theme::TEXT2));
                        }
                    });
            });

        // 版本只读查看
        if let Some((id, src)) = self.view_source.clone() {
            let mut open = true;
            let mut closed = !open;
            egui::Window::new(format!("只读策略 v{id}"))
                .open(&mut open)
                .default_width(560.0)
                .show(ctx, |ui| {
                    ui.label(mono(&src, 11.0, theme::TEXT2));
                });
            closed = closed || !open;
            if closed {
                self.view_source = None;
            }
        }

        // 新建研究项目
        if self.new_project_open {
            let mut open = true;
            egui::Window::new("新建研究项目")
                .open(&mut open)
                .default_width(380.0)
                .show(ctx, |ui| {
                    ui.label(field("项目名称"));
                    let mut name = self.new_project_name.clone();
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut name)
                            .hint_text("例如：低波动轮动研究")
                            .desired_width(ui.available_width()),
                    );
                    if resp.changed() {
                        self.new_project_name = name;
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::primary_button("创建项目")).clicked() {
                            let name = self.new_project_name.trim().to_string();
                            if name.is_empty() {
                                self.ai_notice = Some("请输入项目名称".into());
                            } else {
                                let id = self.workspace.add_project(name);
                                let n = self.workspace.current().messages.len();
                                self.chat_cards.push((id, n - 1, Route::Requirements));
                                self.tell(
                                    format!("已新建项目：会话、需求与版本与原项目互不串扰。"),
                                    None,
                                );
                                self.new_project_open = false;
                                self.new_project_name.clear();
                            }
                        }
                        ui.add_space(6.0);
                        if ui.add(theme::ghost_button("取消")).clicked() {
                            self.new_project_open = false;
                        }
                    });
                });
            if !open {
                self.new_project_open = false;
            }
        }
    }
}

// ================================================================ 业务动作

impl ResearchApp {
    /// 活跃任务计数（四处提交类任务）。
    fn active_task_count(&self) -> usize {
        [
            &self.snapshot_page.watch,
            &self.universe_page.watch,
            &self.plan_page.watch,
            &self.ai_run.watch,
        ]
        .iter()
        .filter(|w| w.is_active())
        .count()
    }

    /// 第一个活跃任务（对话区任务条：任务 ID + 提示 + 轮询键）。
    fn active_task(&self) -> Option<(&str, &'static str, PageKey)> {
        let watches = [
            (&self.ai_run.watch, PageKey::Run),
            (&self.snapshot_page.watch, PageKey::Snapshot),
            (&self.universe_page.watch, PageKey::Universe),
            (&self.plan_page.watch, PageKey::Plan),
        ];
        watches
            .iter()
            .find_map(|(w, key)| {
                w.task_id().and_then(|id| match w {
                    TaskWatch::Submitted { label, .. } => Some((id, *label, *key)),
                    _ => None,
                })
            })
            .or_else(|| {
                watches.iter().find_map(|(w, key)| match w {
                    TaskWatch::Submitted { .. } => w.task_id().map(|id| (id, "任务执行中", *key)),
                    _ => None,
                })
            })
    }

    /// 页面状态访问（只读）。
    fn page_state(&self, key: PageKey) -> &PageState<TaskWatch> {
        match key {
            PageKey::Snapshot => &self.snapshot_page,
            PageKey::Universe => &self.universe_page,
            PageKey::Plan => &self.plan_page,
            PageKey::Run => &self.ai_run,
        }
    }

    /// 页面状态写回。
    fn set_page_state(&mut self, key: PageKey, state: PageState<TaskWatch>) {
        match key {
            PageKey::Snapshot => self.snapshot_page = state,
            PageKey::Universe => self.universe_page = state,
            PageKey::Plan => self.plan_page = state,
            PageKey::Run => self.ai_run = state,
        }
    }

    /// 终态错误行写回指定页。
    fn note_page_error(&mut self, key: PageKey, text: String) {
        let mut state = self.page_state(key).clone();
        state.note_terminal_error(text);
        self.set_page_state(key, state);
    }

    /// 轮询某页任务：get_task → 状态机推进（终态落定并停止轮询）。
    fn poll(&mut self, key: PageKey) {
        let watch = self.page_state(key).watch.clone();
        if !watch.is_active() {
            return;
        }
        let Some(task_id) = watch.task_id().map(str::to_string) else {
            return;
        };
        let Some(bridge) = &self.bridge else { return };
        let Ok(view) = bridge.get_task(&task_id) else {
            return;
        };
        let failed = terminal_error_text(&view);
        let mut state = self.page_state(key).clone();
        if state.watch.poll(view) {
            if let Some(text) = failed {
                state.note_terminal_error(text);
            }
            self.set_page_state(key, state);
        }
        // 实验任务终态后如需回填收益，由后续协调器指标读取承载（当前诚实留空）
    }

    /// 对话发送：意图路由 → 预设回复 → 路由跳转（Unknown 停留并诚实解释）。
    fn do_ai_send_text(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        let intent = ai::route(&text);
        let reply = ai::reply(&text);
        let (pid, assistant_index) = {
            let p = self.workspace.current_mut();
            p.messages.push(crate::workspace::ChatMessage {
                role: crate::workspace::Role::User,
                text,
            });
            p.messages.push(crate::workspace::ChatMessage {
                role: crate::workspace::Role::Assistant,
                text: reply,
            });
            (p.id, p.messages.len() - 1)
        };
        if intent != ai::Intent::Unknown {
            if let Some(route) = Route::from_intent(intent) {
                self.chat_cards.push((pid, assistant_index, route));
                let from = self.session.route;
                self.session.navigate(route, self.session.scroll_of(from));
            }
        }
        self.chat_pin = true;
    }

    /// 追加助手消息 + 运行记录（产物卡可选）。
    fn tell(&mut self, text: impl Into<String>, card: Option<Route>) {
        let text = text.into();
        let first_line = text.lines().next().unwrap_or_default().to_string();
        let (pid, idx) = {
            let p = self.workspace.current_mut();
            p.messages.push(crate::workspace::ChatMessage {
                role: crate::workspace::Role::Assistant,
                text,
            });
            (p.id, p.messages.len() - 1)
        };
        if let Some(r) = card {
            self.chat_cards.push((pid, idx, r));
        }
        self.chat_pin = true;
        self.record(&first_line);
    }

    /// 运行记录（时间戳 + 文本）。
    fn record(&mut self, text: &str) {
        let ts = nautilus_research_domain::time::now_rfc3339();
        let hhmmss = ts.get(11..19).unwrap_or("").to_string();
        self.logs.push(format!("{hhmmss} · {text}"));
    }

    // ---- 数据中心
    fn do_import(&mut self) {
        let Some(bridge) = &self.bridge else { return };
        let form = self.import_form.clone();
        match bridge.submit_import(&form) {
            Ok(r) => {
                self.snapshot_page.on_submitted(r.task_id, "导入中…");
                self.record("导入数据 · 已提交");
            }
            Err(e) => {
                self.snapshot_page
                    .on_submit_failed(format!("导入被拒绝{}", crate::pipeline::rejection_text(&e)));
                self.record("导入数据 · 提交被拒绝");
            }
        }
    }

    // ---- 股票池
    fn do_preview(&mut self) {
        let Some((bridge, snapshot_id)) = self.pick_snapshot() else {
            self.universe_page
                .on_submit_failed("尚无数据快照：请先在数据中心导入");
            return;
        };
        let form = self.universe_form.clone();
        match bridge.submit_preview(&snapshot_id, &form) {
            Ok(r) => {
                self.universe_page.on_submitted(r.task_id, "预览中…");
                self.record("股票池预览 · 已提交");
            }
            Err(e) => self
                .universe_page
                .on_submit_failed(format!("预览被拒绝{}", crate::pipeline::rejection_text(&e))),
        }
    }

    fn do_save_universe(&mut self, preview_task: &str, preview_hash: &str, input_hash: &str) {
        let Some(bridge) = &self.bridge else { return };
        match bridge.save_universe(preview_task, preview_hash, input_hash, "GUI 保存") {
            Ok(u) => {
                self.universe_saved = Some(u);
                self.record("股票池版本 · 已保存");
            }
            Err(e) => self
                .universe_page
                .note_terminal_error(format!("保存失败{}", crate::pipeline::rejection_text(&e))),
        }
    }

    // ---- 实验运行（冻结当前版本 → 真实协调器执行）
    fn do_ai_run(&mut self) {
        let exp_id = match self.workspace.run_experiment(None) {
            Ok(id) => id,
            Err(e) => {
                self.ai_notice = Some(e);
                return;
            }
        };
        self.ai_experiment = Some(exp_id);
        let Some(snapshot_id) = self.pick_snapshot().map(|(_, id)| id) else {
            self.ai_run
                .on_submit_failed("尚无数据快照：请先在数据中心导入");
            return;
        };
        let Some(u) = &self.universe_saved else {
            self.ai_run
                .on_submit_failed("尚无股票池版本：请先在股票池保存");
            return;
        };
        let universe_id = u.universe_id.clone();
        let form = self.run_form.clone();
        let Some(bridge) = &self.bridge else { return };
        match bridge.submit_run(&snapshot_id, &universe_id, &form) {
            Ok(r) => {
                let task_id = r.task_id.clone();
                self.ai_run.on_submitted(r.task_id, "实验运行中…");
                self.workspace.attach_task(exp_id, task_id);
                self.record(&format!("运行实验 E{exp_id} · 已提交"));
            }
            Err(e) => {
                self.ai_run
                    .on_submit_failed(format!("运行被拒绝{}", crate::pipeline::rejection_text(&e)));
                self.record("运行实验 · 提交被拒绝");
            }
        }
    }

    fn do_cancel(&mut self, task_id: String, key: PageKey) {
        let Some(bridge) = &self.bridge else { return };
        match bridge.cancel(&task_id) {
            Ok(_) => self.record("任务取消 · 已请求"),
            Err(e) => self.note_page_error(
                key,
                format!("取消失败{}", crate::pipeline::rejection_text(&e)),
            ),
        }
    }

    // ---- 比较
    fn do_compare(&mut self) {
        self.compare_error = None;
        let ids: Vec<String> = self
            .compare_form
            .run_ids_text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        let Some(bridge) = &self.bridge else {
            self.compare_error = Some("工作区未连接".into());
            return;
        };
        let form = self.compare_form.clone();
        match bridge.compare(&form, ids) {
            Ok(cmp) => {
                self.compare_result = Some(cmp);
                self.record("运行比较 · 完成");
            }
            Err(e) => {
                self.compare_error =
                    Some(format!("比较被拒绝{}", crate::pipeline::rejection_text(&e)))
            }
        }
    }

    // ---- 验证报告
    fn do_make_report(&mut self) {
        match self.workspace.make_report() {
            Ok(vid) => self.tell(
                format!(
                    "验证报告已生成（v{vid}）：演示检查以实验证据为准；正式策略验证仍为证据不足——真实数据、样本外与参数稳健性未运行。"
                ),
                Some(Route::Validate),
            ),
            Err(e) => self.ai_notice = Some(e),
        }
    }

    // ---- 协调器计划（生产对接）
    fn do_plan(&mut self) {
        let Some((bridge, snapshot_id)) = self.pick_snapshot() else {
            self.plan_page
                .on_submit_failed("尚无数据快照：请先在数据中心导入");
            return;
        };
        let Some(u) = &self.universe_saved else {
            self.plan_page
                .on_submit_failed("尚无股票池版本：请先在股票池保存");
            return;
        };
        let universe_id = u.universe_id.clone();
        let (form, run_form) = (self.plan_form.clone(), self.run_form.clone());
        match bridge.submit_plan(&snapshot_id, &universe_id, &form, &run_form) {
            Ok(r) => {
                self.plan_page.on_submitted(r.task_id, "计划生成中…");
                self.record("生成计划（协调器）· 已提交");
            }
            Err(e) => self
                .plan_page
                .on_submit_failed(format!("计划被拒绝{}", crate::pipeline::rejection_text(&e))),
        }
    }

    fn do_export(&mut self, plan_id: &str) {
        let Some(bridge) = &self.bridge else { return };
        let dest = self.plan_form.export_path.clone();
        match bridge.export_plan(plan_id, &dest) {
            Ok(r) => {
                self.plan_exported = Some(format!(
                    "已导出 {}（sha256 {}…，{} 行）",
                    r.path,
                    &r.sha256[..12],
                    r.rows
                ));
                self.record("导出计划 CSV · 完成");
            }
            Err(e) => self
                .plan_page
                .note_terminal_error(format!("导出失败{}", crate::pipeline::rejection_text(&e))),
        }
    }

    fn do_note(&mut self, plan_id: &str) {
        let Some(bridge) = &self.bridge else { return };
        let text = self.plan_form.note_text.clone();
        match bridge.save_note(plan_id, &text, "备注") {
            Ok(_) => {
                self.plan_exported = Some("备注已保存".into());
                self.record("计划备注 · 已保存");
            }
            Err(e) => self
                .plan_page
                .note_terminal_error(format!("备注失败{}", crate::pipeline::rejection_text(&e))),
        }
    }

    // ---- 需求 / 设计 / 版本（研究工作区状态机）
    fn do_confirm_requirement(&mut self) {
        let min_amount = self.ai_req_min_amount.trim().parse::<f64>().unwrap_or(-1.0);
        let (text, acceptance, alloc) = (
            self.ai_req_text.clone(),
            self.ai_req_acceptance.clone(),
            self.ai_req_alloc,
        );
        match self
            .workspace
            .confirm_requirement(&text, &acceptance, alloc, min_amount)
        {
            Ok(id) => {
                self.tell(
                    format!("需求 R{id} 已确认。下一步保存设计说明。旧实验保留，新研究将使用此需求版本。"),
                    Some(Route::Design),
                );
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    fn do_generate_design(&mut self) {
        let note_text = self.ai_design_note.clone();
        match self.workspace.generate_design(note_text) {
            Ok(id) => {
                let req = self.workspace.current().active_req.unwrap_or_default();
                self.tell(
                    format!("设计 D{id} 已保存，绑定 R{req}。处理图与代码将引用相同的节点和参数。"),
                    Some(Route::Develop),
                );
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    fn do_save_version(&mut self) {
        let src = self.ai_code_source.clone();
        match self.workspace.save_version(src.clone()) {
            Ok(id) => {
                let (req, design) = {
                    let p = self.workspace.current();
                    let v = &p.versions[id - 1];
                    (v.req_id, v.design_id)
                };
                self.tell(
                    format!("策略 v{id} 已保存，冻结 R{req} / D{design} 引用与输入，不可变。可以开始实验。"),
                    Some(Route::Experiments),
                );
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    // ---- 计划桥（S20）
    fn do_ai_plan_generate(&mut self) {
        let Some(snapshot_id) = self.pick_snapshot().map(|(_, id)| id) else {
            self.ai_notice = Some("尚无数据快照：请先在数据中心导入".into());
            return;
        };
        let (cash, total, rows) = match crate::bridge::parse_plan_rows(&self.ai_plan_json) {
            Ok(x) => x,
            Err(e) => {
                self.ai_notice = Some(e);
                return;
            }
        };
        let date = self.ai_plan_trade_date.trim().to_string();
        if date.is_empty() {
            self.ai_notice = Some("请填写交易日".into());
            return;
        }
        match self
            .workspace
            .make_plan(snapshot_id, date, cash, total, rows)
        {
            Ok(()) => {
                self.ai_plan_issues = None;
                self.ai_plan_csv = None;
                self.tell(
                    "计划草稿已生成：账户与数据输入已冻结为签名。请核对可卖数量、买入费用和数据有效期后确认导出。".to_string(),
                    None,
                );
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    fn do_ai_plan_check(&mut self) {
        // 买入含费合计：Σ 目标买入金额 ×（1 + 佣金率）——费率取运行参数
        let rate = self
            .run_form
            .commission_rate
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0);
        let cost = self
            .workspace
            .current()
            .plan
            .as_ref()
            .map(|t| {
                t.rows
                    .iter()
                    .filter(|r| r.5 > 0)
                    .map(|r| r.1 * r.5 as f64 * (1.0 + rate))
                    .sum::<f64>()
            })
            .unwrap_or(0.0);
        match self.workspace.check_plan(cost) {
            Ok(issues) => {
                self.ai_plan_csv = None;
                self.ai_plan_issues = Some(issues);
                if self.ai_plan_issues.as_ref().is_some_and(|i| i.is_empty()) {
                    self.record("计划核对 · 通过");
                } else {
                    self.record("计划核对 · 未通过");
                }
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    /// 确认导出：按当前表单输入重算签名——生成后输入未变才放行（S20 门控）。
    fn do_ai_plan_export(&mut self) {
        let (draft_snapshot, version_id) = {
            let p = self.workspace.current();
            let Some(t) = &p.plan else {
                self.ai_notice = Some("尚未生成计划".into());
                return;
            };
            (t.snapshot.clone(), p.active_version)
        };
        let Some(version_id) = version_id else {
            self.ai_notice = Some("当前版本不存在".into());
            return;
        };
        let Ok((cash, _total, rows)) = crate::bridge::parse_plan_rows(&self.ai_plan_json) else {
            self.ai_notice = Some("持仓 JSON 已不可解析，无法确认导出".into());
            return;
        };
        let sig = crate::workspace::plan_signature(
            version_id,
            &draft_snapshot,
            self.ai_plan_trade_date.trim(),
            cash,
            &rows,
        );
        match self.workspace.confirm_export(sig) {
            Ok(csv) => {
                // 写入工作区导出目录（人工交接；不发送任何订单）
                let path = self
                    .bridge
                    .as_ref()
                    .map(|b| b.workspace.join("研序-交易计划.csv"))
                    .unwrap_or_else(|| std::path::PathBuf::from("研序-交易计划.csv"));
                match std::fs::write(&path, csv.as_bytes()) {
                    Ok(()) => {
                        self.tell(format!("演示计划已导出：{}。文件包含策略版本、交易日与调整数量；没有发送任何订单。", path.display()), None)
                    }
                    Err(e) => self.ai_notice = Some(format!("文件写入失败：{e}（CSV 内容保留在页面下方）")),
                }
                self.ai_plan_csv = Some(csv);
            }
            Err(e) => {
                self.ai_plan_csv = None;
                self.ai_notice = Some(e);
            }
        }
    }

    /// 取最新快照 ID（页面间流转的当前输入）。
    fn pick_snapshot(&self) -> Option<(&DesktopBridge, String)> {
        let bridge = self.bridge.as_ref()?;
        let page = bridge.snapshots(1).ok()?;
        let latest = page.items.first()?.snapshot_id.clone();
        Some((bridge, latest))
    }
}

/// 报告是否可用（当前版本 + 新鲜 + 演示检查通过）。
fn report_ready(w: &Workspace) -> bool {
    let p = w.current();
    p.report.as_ref().is_some_and(|r| {
        r.version_id == p.active_version.unwrap_or(0)
            && w.version_fresh(p.active_version.unwrap_or(0))
            && r.demo_pass()
    })
}

// ================================================================ 绘制辅助

/// `allocate_ui` 期望尺寸：极端窄窗（首帧守卫之外的双保险）下把负宽/高钳 0——
/// egui 对负 desired size 直接断言 panic（desktop-firstframe-guard）。
fn sz(w: f32, h: f32) -> egui::Vec2 {
    egui::vec2(w.max(0.0), h.max(0.0))
}

/// 正文文字。
fn lbl(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text.into())
        .font(FontId::proportional(size))
        .color(color)
}

/// 等宽文字（数字与代码）。
fn mono(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text.into())
        .font(FontId::monospace(size))
        .color(color)
}

/// 表单字段标签。
fn field(text: impl Into<String>) -> egui::RichText {
    lbl(text, 12.0, theme::TEXT2)
}

/// 说明文字。
fn note(text: impl Into<String>) -> egui::RichText {
    lbl(text, 11.0, theme::MUTED)
}

/// 概念文字。
fn concepts(text: impl Into<String>) -> egui::RichText {
    lbl(text, 11.0, theme::MUTED)
}

/// tab 小按钮（选中 = 主描边）。
fn tab_button(text: &str, active: bool) -> egui::Button<'_> {
    let t = lbl(
        text,
        10.0,
        if active {
            theme::ACCENT_TEXT
        } else {
            theme::MUTED
        },
    );
    egui::Button::new(t)
        .fill(theme::GLASS)
        .stroke(if active {
            Stroke::new(1.0, Color32::from_rgba_premultiplied(8, 26, 15, 102))
        } else {
            Stroke::new(1.0, theme::BORDER_STRONG)
        })
        .corner_radius(CornerRadius::same(7))
}

/// 页头：eyebrow + 标题 + 副标题。
fn page_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
    ui.label(mono(eyebrow, 9.0, theme::FAINT));
    ui.add_space(6.0);
    ui.label(lbl(title, 24.0, theme::TEXT));
    ui.add_space(8.0);
    ui.label(lbl(subtitle, 11.0, theme::MUTED));
    ui.add_space(20.0);
}

/// 玻璃面板：标题头（含徽章）+ 内边距内容。
fn panel(
    ui: &mut egui::Ui,
    title: &str,
    badge: Option<(&str, TagKind)>,
    body: impl FnOnce(&mut egui::Ui),
) {
    Frame::NONE
        .fill(theme::GLASS)
        .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let head = Frame::NONE
                .inner_margin(Margin {
                    left: 19,
                    right: 19,
                    top: 14,
                    bottom: 14,
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(lbl(title, 12.0, theme::TEXT2));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if let Some((t, k)) = badge {
                                theme::tag_ui(ui, t, k);
                            }
                        });
                    });
                });
            let hr = head.response.rect;
            ui.painter().line_segment(
                [
                    Pos2::new(hr.left(), hr.bottom()),
                    Pos2::new(hr.right(), hr.bottom()),
                ],
                Stroke::new(1.0, theme::BORDER),
            );
            Frame::NONE.inner_margin(Margin::same(19)).show(ui, body);
        });
    ui.add_space(12.0);
}

/// 空态面板（可选主按钮；返回按钮是否被点击）。
fn empty_panel(ui: &mut egui::Ui, title: &str, desc: &str, action: Option<&str>) -> bool {
    let mut clicked = false;
    Frame::NONE
        .fill(theme::GLASS)
        .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin::same(22))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                // 原型 .empty .icon：30px flow 图标（#3d5c4a 深绿）
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(30.0), Sense::hover());
                icons::paint_icon(
                    ui.painter(),
                    icon_rect,
                    icons::IconKind::Flow,
                    Color32::from_rgb(0x3D, 0x5C, 0x4A),
                );
                ui.add_space(10.0);
                ui.label(lbl(title, 15.0, theme::TEXT));
                ui.add_space(8.0);
                ui.label(lbl(desc, 12.0, theme::MUTED));
                if let Some(a) = action {
                    ui.add_space(14.0);
                    if ui.add(theme::primary_button(a)).clicked() {
                        clicked = true;
                    }
                }
                ui.add_space(8.0);
            });
        });
    ui.add_space(12.0);
    clicked
}

/// 键值行（105px 键列 + 值列）。
fn kv(ui: &mut egui::Ui, rows: &[(&str, String)]) {
    egui::Grid::new("kv-grid")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.label(lbl(*k, 11.0, theme::FAINT));
                ui.label(lbl(v, 11.0, theme::TEXT2));
                ui.end_row();
            }
        });
}

/// 页面任务观察的通用渲染（状态行 / 错误行）。
fn render_watch(ui: &mut egui::Ui, page: &PageState<TaskWatch>) {
    if let Some(err) = &page.error {
        ui.label(lbl(&err.text, 11.0, theme::RED));
    }
    match &page.watch {
        TaskWatch::Idle => {}
        TaskWatch::Submitted { label, .. } => {
            ui.label(lbl(*label, 11.0, theme::ACCENT_CYAN));
        }
        TaskWatch::Terminal(v) => {
            let ok = crate::pipeline::is_success(v);
            let state = if ok {
                "成功"
            } else {
                match v.state {
                    nautilus_research_domain::TaskState::Cancelled => "已取消",
                    _ => "失败",
                }
            };
            ui.label(lbl(
                format!("任务 {}：{state}", v.task_id),
                11.0,
                if ok { theme::ACCENT_TEXT } else { theme::RED },
            ));
        }
    }
}

fn fmt_metric(m: &nautilus_research_domain::metrics::NullableMetric) -> String {
    m.value.clone().unwrap_or_else(|| "—".into())
}

/// 文本截断（对话区任务条等窄场景）。
fn ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// RichText 对齐辅助（painter.text 用）。
const ALIGN2_CENTER: egui::Align2 = egui::Align2::CENTER_CENTER;

#[cfg(test)]
mod tests {
    use super::*;

    /// 概览下一步推断：链路逐级推进（需求 → 设计 → 版本 → 实验 → 报告 → 计划）。
    /// 内联纯函数单测不带 reporter（惯例：reporter 由 tests/ 规格对齐用例承载）。
    #[test]
    fn next_step_follows_research_chain() {
        let mut w = Workspace::new();
        let (label, route, _) = next_step(&w);
        assert_eq!(
            (label.as_str(), route),
            ("确认研究需求", Route::Requirements)
        );

        w.confirm_requirement("量价", "夏普>0", 30.0, 100.0)
            .unwrap();
        assert_eq!(next_step(&w).1, Route::Design);

        w.generate_design("EMA 双均线").unwrap();
        assert_eq!(next_step(&w).1, Route::Develop);

        w.save_version("fn strategy() {}").unwrap();
        let (label, route, reason) = next_step(&w);
        assert_eq!(route, Route::Experiments);
        assert!(reason.contains("尚无实验"), "{reason}");
        let _ = label;

        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        assert_eq!(next_step(&w).1, Route::Validate);

        w.make_report().unwrap();
        assert_eq!(next_step(&w).1, Route::Plan);

        // 计划生成 → 未核对 → 核对
        let rows = vec![("SYN-A".to_string(), 10.0, 1000, 1000, 1100, 100)];
        w.make_plan("SNAP-1", "2024-01-05", 10000.0, 20000.0, rows)
            .unwrap();
        assert_eq!(next_step(&w).1, Route::Plan);
        let checked = w.check_plan(1500.0).unwrap();
        assert!(checked.is_empty());
        let (label, _, reason) = next_step(&w);
        assert_eq!(label, "确认导出交易清单");
        assert!(reason.contains("证据不足"));
        let _ = e1;
    }
}
