//! eframe App：六页导航 + 玻璃态面板 + 按需重绘 + 流水线五页协调器对接。
//!
//! 按需重绘约束（验收红线：不能打爆 CPU）：静默时不请求重绘（零帧）；
//! 仅当存在活跃任务时以 `request_repaint_after(POLL_INTERVAL)` 定时轮询
//! `get_task`（轻量只读），任务推进到终态立即落定并停止轮询。

use egui::{Color32, CornerRadius, FontId, Frame, Margin, Pos2, Stroke};

use crate::ai::{self, Intent};
use crate::bridge::{CompareForm, DesktopBridge, ImportForm, PlanForm, RunForm, UniverseForm};
use crate::layout;
use crate::nav::{Page, ALL_PAGES};
use crate::pipeline::{terminal_error_text, PageState, TaskWatch, POLL_INTERVAL};
use crate::session::Session;
use crate::theme;
use crate::workspace::Workspace;

/// 提交类页面的轮询键。
#[derive(Debug, Clone, Copy, PartialEq)]
enum PageKey {
    Snapshot,
    Universe,
    Run,
    Plan,
    /// AI 工作台实验运行。
    AiRun,
}

/// AI 工作台四子视图（S17~S20：项目/开发/调试/计划桥）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiView {
    Project,
    Dev,
    Debug,
    PlanBridge,
}

impl AiView {
    pub const ALL: [AiView; 4] = [
        AiView::Project,
        AiView::Dev,
        AiView::Debug,
        AiView::PlanBridge,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "项目",
            Self::Dev => "开发",
            Self::Debug => "调试",
            Self::PlanBridge => "计划桥",
        }
    }

    /// 意图目标页 → 子视图（对话路由跳转）。
    pub fn from_intent(intent: Intent) -> Self {
        match intent.target_page() {
            "开发" => Self::Dev,
            "调试" => Self::Debug,
            "计划桥" => Self::PlanBridge,
            _ => Self::Project,
        }
    }
}

/// 研究桌面应用。
pub struct ResearchApp {
    /// 会话状态（当前页、滚动、字体提示）。
    pub session: Session,
    /// 协调器桥（打开失败时保留错误行，页面提示重试）。
    pub bridge: Option<DesktopBridge>,
    pub bridge_error: Option<String>,

    // 数据快照页
    pub import_form: ImportForm,
    pub snapshot_page: PageState<TaskWatch>,
    pub universe_saved: Option<nautilus_research_domain::protocol::UniverseRef>,

    // 股票池页
    pub universe_form: UniverseForm,
    pub universe_page: PageState<TaskWatch>,

    // 运行页
    pub run_form: RunForm,
    pub run_page: PageState<TaskWatch>,

    // 比较页
    pub compare_form: CompareForm,
    pub compare_error: Option<String>,
    pub compare_result: Option<nautilus_research_domain::protocol::Comparison>,

    // 计划页
    pub plan_form: PlanForm,
    pub plan_page: PageState<TaskWatch>,
    pub plan_exported: Option<String>,

    // AI 工作台页（S17~S20：会话态随应用存活，切页/切项目不丢失）
    pub workspace: Workspace,
    pub ai_view: AiView,
    pub ai_input: String,
    pub ai_notice: Option<String>,
    // 项目视图：需求确认表单
    pub ai_req_text: String,
    pub ai_req_acceptance: String,
    pub ai_req_alloc: f64,
    pub ai_req_min_amount: String,
    // 开发视图：设计说明与代码草稿
    pub ai_design_note: String,
    pub ai_code_source: String,
    // 调试视图：实验任务与当前实验引用
    pub ai_run: PageState<TaskWatch>,
    pub ai_experiment: Option<usize>,
    // 计划桥视图：交易日/持仓 JSON/核对结果/导出产物
    pub ai_plan_trade_date: String,
    pub ai_plan_json: String,
    pub ai_plan_issues: Option<Vec<String>>,
    pub ai_plan_csv: Option<String>,
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
                rule_text: r#"{"op":"and","children":[{"field":"close","op":"gte","value":"0"}]}"#.into(),
                as_of: String::new(),
                strict: true,
                fixed_membership: true,
                ignore_missing: false,
            },
            universe_page: PageState::default(),
            run_form: RunForm::default(),
            run_page: PageState::default(),
            compare_form: CompareForm::default(),
            compare_error: None,
            compare_result: None,
            plan_form: PlanForm::default(),
            plan_page: PageState::default(),
            plan_exported: None,
            workspace: Workspace::new(),
            ai_view: AiView::Project,
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

impl eframe::App for ResearchApp {
    /// 根 Ui 渲染（eframe 0.36 模型）：顶栏 + 左导航 + 底状态 + 中栏页面。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 顶栏（56px）
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
                        let hint = match &self.bridge {
                            Some(b) => format!("工作区：{}", b.workspace.display()),
                            None => self
                                .bridge_error
                                .clone()
                                .unwrap_or_else(|| "工作区：未连接".into()),
                        };
                        ui.label(
                            egui::RichText::new(hint)
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
                        egui::Button::new(label)
                            .fill(fill)
                            .corner_radius(CornerRadius::same(theme::CARD_ROUNDING)),
                    );
                    if active {
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

        // 底部任务状态（32px）：活跃任务计数 + 字体缺失诚实提示
        let active = self.active_task_count();
        egui::Panel::bottom("statusbar")
            .exact_size(layout::BOTTOMBAR_H)
            .frame(
                Frame::NONE
                    .fill(theme::GLASS)
                    .inner_margin(Margin::symmetric(16, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let text = if active > 0 {
                        format!("后台任务：{active} 项进行中（每 {POLL_INTERVAL:?} 刷新）")
                    } else {
                        "后台任务：无".to_string()
                    };
                    ui.label(
                        egui::RichText::new(text)
                            .font(FontId::monospace(11.0))
                            .color(if active > 0 {
                                theme::ACCENT
                            } else {
                                theme::TEXT_DIM
                            }),
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

        // 中栏：当前页
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
                                self.render_page(ui, page);
                            });
                    });
            });

        // 按需重绘：有活跃任务才定时轮询（轻量 get_task 推进状态机）
        if active > 0 {
            ui.ctx().request_repaint_after(POLL_INTERVAL);
        }
        // 否则不请求任何重绘：事件驱动，静默零帧（CPU 红线）
    }
}

impl ResearchApp {
    /// 活跃任务计数（五处提交类任务）。
    fn active_task_count(&self) -> usize {
        [
            &self.snapshot_page.watch,
            &self.universe_page.watch,
            &self.run_page.watch,
            &self.plan_page.watch,
            &self.ai_run.watch,
        ]
        .iter()
        .filter(|w| w.is_active())
        .count()
    }

    /// 页面状态访问（只读）。
    fn page_state(&self, key: PageKey) -> &PageState<TaskWatch> {
        match key {
            PageKey::Snapshot => &self.snapshot_page,
            PageKey::Universe => &self.universe_page,
            PageKey::Run => &self.run_page,
            PageKey::Plan => &self.plan_page,
            PageKey::AiRun => &self.ai_run,
        }
    }

    /// 页面状态写回。
    fn set_page_state(&mut self, key: PageKey, state: PageState<TaskWatch>) {
        match key {
            PageKey::Snapshot => self.snapshot_page = state,
            PageKey::Universe => self.universe_page = state,
            PageKey::Run => self.run_page = state,
            PageKey::Plan => self.plan_page = state,
            PageKey::AiRun => self.ai_run = state,
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
    }

    fn render_page(&mut self, ui: &mut egui::Ui, page: Page) {
        match page {
            Page::DataSnapshot => self.render_snapshots(ui),
            Page::Universe => self.render_universe(ui),
            Page::Runs => self.render_runs(ui),
            Page::Compare => self.render_compare(ui),
            Page::Plan => self.render_plan(ui),
            Page::AiWorkspace => self.render_ai_workspace(ui),
        }
    }

    // ------------------------------------------------ 数据快照页
    fn render_snapshots(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("数据快照").color(theme::TEXT));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("导入暂存数据（每行一个 CSV 路径）").color(theme::TEXT_DIM),
        );
        let mut edited = self.import_form.paths.join("\n");
        let response = ui.add(
            egui::TextEdit::multiline(&mut edited)
                .desired_rows(3)
                .desired_width(560.0),
        );
        if response.changed() {
            self.import_form.paths = edited
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
        }
        ui.horizontal(|ui| {
            ui.label("价格口径：");
            for (i, name) in ["raw", "qfq", "hfq"].iter().enumerate() {
                if ui.radio(self.import_form.price_basis == i, *name).clicked() {
                    self.import_form.price_basis = i;
                }
            }
        });
        let enabled = self.bridge.is_some() && !self.snapshot_page.watch.is_active();
        ui.add_enabled_ui(enabled, |ui| {
            if ui
                .button(egui::RichText::new("导入").color(theme::ACCENT))
                .clicked()
            {
                self.do_import();
            }
        });
        render_watch(ui, &self.snapshot_page);

        ui.add_space(12.0);
        ui.label(egui::RichText::new("已保存快照").color(theme::TEXT_DIM));
        if let Some(bridge) = &self.bridge {
            if let Ok(p) = bridge.snapshots(20) {
                for s in &p.items {
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  as_of={}  清单 {}…",
                            s.snapshot_id,
                            s.as_of,
                            &s.manifest_hash[..12.min(s.manifest_hash.len())]
                        ))
                        .font(FontId::monospace(12.0))
                        .color(theme::TEXT),
                    );
                }
            }
        }
        self.poll(PageKey::Snapshot);
    }

    fn do_import(&mut self) {
        let Some(bridge) = &self.bridge else { return };
        let form = self.import_form.clone();
        match bridge.submit_import(&form) {
            Ok(r) => self.snapshot_page.on_submitted(r.task_id, "导入中…"),
            Err(e) => self
                .snapshot_page
                .on_submit_failed(format!("导入被拒绝{}", crate::pipeline::rejection_text(&e))),
        }
    }

    // ------------------------------------------------ 股票池页
    fn render_universe(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("股票池").color(theme::TEXT));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("规则 JSON（RuleAST）").color(theme::TEXT_DIM),
        );
        let mut rule = self.universe_form.rule_text.clone();
        let response = ui.add(
            egui::TextEdit::multiline(&mut rule)
                .desired_rows(4)
                .desired_width(560.0)
                .font(FontId::monospace(12.0)),
        );
        if response.changed() {
            self.universe_form.rule_text = rule;
        }
        ui.horizontal(|ui| {
            ui.label("as_of：");
            let mut as_of = self.universe_form.as_of.clone();
            if ui.text_edit_singleline(&mut as_of).changed() {
                self.universe_form.as_of = as_of;
            }
            ui.checkbox(&mut self.universe_form.strict, "严格模式");
            ui.checkbox(&mut self.universe_form.fixed_membership, "固定成员");
        });
        let enabled = self.bridge.is_some() && !self.universe_page.watch.is_active();
        ui.add_enabled_ui(enabled, |ui| {
            if ui
                .button(egui::RichText::new("预览").color(theme::ACCENT))
                .clicked()
            {
                self.do_preview();
            }
        });
        render_watch(ui, &self.universe_page);

        // 预览成功：展示三态计数并允许保存
        if let Some(v) = self.universe_page.watch.terminal() {
            if crate::pipeline::is_success(v) {
                if let Some(bridge) = &self.bridge {
                    let task_id = self
                        .universe_page
                        .watch
                        .task_id()
                        .map(str::to_string);
                    if let Some(task_id) = task_id {
                        if let Ok(p) = bridge.preview(&task_id) {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "通过 {} / 排除 {} / 未知 {}",
                                    p.pass, p.exclude, p.unknown
                                ))
                                .color(theme::TEXT),
                            );
                            if ui.button("保存股票池版本").clicked() {
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
            ui.label(
                egui::RichText::new(format!(
                    "已保存：{}（成员 {}）",
                    u.universe_id, u.count
                ))
                .color(theme::ACCENT),
            );
        }
        self.poll(PageKey::Universe);
    }

    fn do_preview(&mut self) {
        let Some((bridge, snapshot_id)) = self.pick_snapshot() else {
            self.universe_page
                .on_submit_failed("尚无数据快照：请先在数据快照页导入");
            return;
        };
        let form = self.universe_form.clone();
        match bridge.submit_preview(&snapshot_id, &form) {
            Ok(r) => self.universe_page.on_submitted(r.task_id, "预览中…"),
            Err(e) => self
                .universe_page
                .on_submit_failed(format!("预览被拒绝{}", crate::pipeline::rejection_text(&e))),
        }
    }

    fn do_save_universe(&mut self, preview_task: &str, preview_hash: &str, input_hash: &str) {
        let Some(bridge) = &self.bridge else { return };
        match bridge.save_universe(preview_task, preview_hash, input_hash, "GUI 保存") {
            Ok(u) => self.universe_saved = Some(u),
            Err(e) => self
                .universe_page
                .note_terminal_error(format!("保存失败{}", crate::pipeline::rejection_text(&e))),
        }
    }

    // ------------------------------------------------ 运行页
    fn render_runs(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("运行").color(theme::TEXT));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("EMA 模板回测（严格模式）").color(theme::TEXT_DIM),
        );
        ui.horizontal(|ui| {
            ui.label("fast");
            if ui.add(egui::DragValue::new(&mut self.run_form.fast)).changed() {}
            ui.label("slow");
            if ui.add(egui::DragValue::new(&mut self.run_form.slow)).changed() {}
            ui.label("top_k");
            if ui.add(egui::DragValue::new(&mut self.run_form.top_k)).changed() {}
        });
        ui.horizontal(|ui| {
            ui.label("起止：");
            let mut s = self.run_form.start.clone();
            let mut e = self.run_form.end.clone();
            if ui.text_edit_singleline(&mut s).changed() {
                self.run_form.start = s;
            }
            if ui.text_edit_singleline(&mut e).changed() {
                self.run_form.end = e;
            }
            ui.label("资金：");
            let mut c = self.run_form.capital.clone();
            if ui.text_edit_singleline(&mut c).changed() {
                self.run_form.capital = c;
            }
        });
        ui.collapsing("成本假设", |ui| {
            for (name, field) in [
                ("佣金率", 0u8),
                ("最低佣金", 1),
                ("卖出税", 2),
                ("其他费", 3),
                ("滑点 bps", 4),
                ("参与率", 5),
            ] {
                ui.horizontal(|ui| {
                    ui.label(name);
                    let mut v = match field {
                        0 => self.run_form.commission_rate.clone(),
                        1 => self.run_form.min_commission.clone(),
                        2 => self.run_form.sell_tax.clone(),
                        3 => self.run_form.other_fee.clone(),
                        4 => self.run_form.slippage_bps.clone(),
                        _ => self.run_form.participation.clone(),
                    };
                    if ui.text_edit_singleline(&mut v).changed() {
                        match field {
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
        let enabled = self.bridge.is_some() && !self.run_page.watch.is_active();
        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button(egui::RichText::new("提交运行").color(theme::ACCENT))
                    .clicked()
                {
                    self.do_run();
                }
                if self.run_page.watch.is_active() {
                    if let Some(task_id) = self.run_page.watch.task_id() {
                        let task_id = task_id.to_string();
                        if ui
                            .button(egui::RichText::new("取消").color(theme::DANGER))
                            .clicked()
                        {
                            self.do_cancel(task_id, PageKey::Run);
                        }
                    }
                }
            });
        });
        render_watch(ui, &self.run_page);
        self.poll(PageKey::Run);
    }

    fn do_run(&mut self) {
        let Some((bridge, snapshot_id)) = self.pick_snapshot() else {
            self.run_page
                .on_submit_failed("尚无数据快照：请先在数据快照页导入");
            return;
        };
        let Some(u) = &self.universe_saved else {
            self.run_page
                .on_submit_failed("尚无股票池版本：请先在股票池页保存");
            return;
        };
        let universe_id = u.universe_id.clone();
        let form = self.run_form.clone();
        match bridge.submit_run(&snapshot_id, &universe_id, &form) {
            Ok(r) => self.run_page.on_submitted(r.task_id, "回测运行中…"),
            Err(e) => self
                .run_page
                .on_submit_failed(format!("运行被拒绝{}", crate::pipeline::rejection_text(&e))),
        }
    }

    fn do_cancel(&mut self, task_id: String, key: PageKey) {
        let Some(bridge) = &self.bridge else { return };
        match bridge.cancel(&task_id) {
            Ok(_) => {}
            Err(e) => self.note_page_error(
                key,
                format!("取消失败{}", crate::pipeline::rejection_text(&e)),
            ),
        }
    }

    // ------------------------------------------------ 比较页
    fn render_compare(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("比较").color(theme::TEXT));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("2..5 个运行 ID（逗号分割；ID 见运行页任务）")
                .color(theme::TEXT_DIM),
        );
        let mut text = self.compare_form.run_ids_text.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut text)
                    .desired_width(560.0)
                    .font(FontId::monospace(12.0)),
            )
            .changed()
        {
            self.compare_form.run_ids_text = text;
        }
        ui.checkbox(&mut self.compare_form.intersection, "交集视图");
        if ui
            .button(egui::RichText::new("比较").color(theme::ACCENT))
            .clicked()
        {
            self.do_compare();
        }
        if let Some(err) = &self.compare_error {
            ui.label(egui::RichText::new(err).color(theme::DANGER));
        }
        if let Some(cmp) = &self.compare_result {
            ui.add_space(8.0);
            egui::Grid::new("compare-grid").show(ui, |ui| {
                for head in ["运行", "区间", "总收益", "夏普", "回撤"] {
                    ui.label(egui::RichText::new(head).color(theme::TEXT_DIM));
                }
                ui.end_row();
                for r in &cmp.runs {
                    let m = &r.metrics;
                    ui.label(
                        egui::RichText::new(&r.run_id[..12.min(r.run_id.len())])
                            .font(FontId::monospace(12.0)),
                    );
                    ui.label(format!("{}~{}", r.start, r.end));
                    ui.label(fmt_metric(&m.total_return));
                    ui.label(fmt_metric(&m.sharpe));
                    ui.label(fmt_metric(&m.max_drawdown));
                    ui.end_row();
                }
            });
            for d in &cmp.differences {
                ui.label(
                    egui::RichText::new(format!("差异 [{}] {}", d.kind, d.detail))
                        .color(theme::ACCENT_CYAN),
                );
            }
        }
    }

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
            Ok(cmp) => self.compare_result = Some(cmp),
            Err(e) => {
                self.compare_error =
                    Some(format!("比较被拒绝{}", crate::pipeline::rejection_text(&e)))
            }
        }
    }

    // ------------------------------------------------ 计划页
    fn render_plan(&mut self, ui: &mut egui::Ui) {
        ui.heading(egui::RichText::new("计划").color(theme::TEXT));
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("生成交易计划（供人工核对；无自动下单入口）")
                .color(theme::TEXT_DIM),
        );
        ui.horizontal(|ui| {
            ui.label("as_of：");
            let mut as_of = self.plan_form.as_of.clone();
            if ui.text_edit_singleline(&mut as_of).changed() {
                self.plan_form.as_of = as_of;
            }
        });
        ui.label(
            egui::RichText::new("手工持仓 JSON（空 = 使用最近持有版本）")
                .color(theme::TEXT_DIM),
        );
        let mut holdings = self.plan_form.holdings_json.clone();
        if ui
            .add(
                egui::TextEdit::multiline(&mut holdings)
                    .desired_rows(3)
                    .desired_width(560.0)
                    .font(FontId::monospace(12.0)),
            )
            .changed()
        {
            self.plan_form.holdings_json = holdings;
        }
        let enabled = self.bridge.is_some() && !self.plan_page.watch.is_active();
        ui.add_enabled_ui(enabled, |ui| {
            if ui
                .button(egui::RichText::new("生成计划").color(theme::ACCENT))
                .clicked()
            {
                self.do_plan();
            }
        });
        render_watch(ui, &self.plan_page);

        // 终态成功：导出与备注
        if let Some(v) = self.plan_page.watch.terminal() {
            if crate::pipeline::is_success(v) {
                if let Some(plan_id) = self.plan_page.watch.task_id() {
                    let plan_id = plan_id.to_string();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let mut path = self.plan_form.export_path.clone();
                        let resp = ui
                            .add(egui::TextEdit::singleline(&mut path).desired_width(380.0));
                        if resp.changed() {
                            self.plan_form.export_path = path.clone();
                        }
                        if ui.button("导出 CSV").clicked() && !path.is_empty() {
                            self.do_export(&plan_id);
                        }
                    });
                    let mut note = self.plan_form.note_text.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut note).desired_width(380.0))
                        .changed()
                    {
                        self.plan_form.note_text = note;
                    }
                    if ui.button("保存备注").clicked()
                        && !self.plan_form.note_text.is_empty()
                    {
                        self.do_note(&plan_id);
                    }
                }
            }
        }
        if let Some(info) = &self.plan_exported {
            ui.label(egui::RichText::new(info).color(theme::ACCENT));
        }
        self.poll(PageKey::Plan);
    }

    fn do_plan(&mut self) {
        let Some((bridge, snapshot_id)) = self.pick_snapshot() else {
            self.plan_page
                .on_submit_failed("尚无数据快照：请先在数据快照页导入");
            return;
        };
        let Some(u) = &self.universe_saved else {
            self.plan_page
                .on_submit_failed("尚无股票池版本：请先在股票池页保存");
            return;
        };
        let universe_id = u.universe_id.clone();
        let (form, run_form) = (self.plan_form.clone(), self.run_form.clone());
        match bridge.submit_plan(&snapshot_id, &universe_id, &form, &run_form) {
            Ok(r) => self.plan_page.on_submitted(r.task_id, "计划生成中…"),
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
                ))
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
            Ok(_) => self.plan_exported = Some("备注已保存".into()),
            Err(e) => self
                .plan_page
                .note_terminal_error(format!("备注失败{}", crate::pipeline::rejection_text(&e))),
        }
    }

    // ------------------------------------------------ AI 工作台页（S17～S20）
    fn render_ai_workspace(&mut self, ui: &mut egui::Ui) {
        let project = self.workspace.current().name.clone();
        ui.heading(
            egui::RichText::new(format!("AI 工作台 · {project}")).color(theme::TEXT),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(
                "离线预设意图（无 LLM、无网络）：只能执行预设研究动作，未知请求诚实拒绝",
            )
            .color(theme::TEXT_DIM),
        );

        // 对话区（仅渲染最近 20 条，避免长会话拖慢帧；完整历史留在会话态）
        let msgs: Vec<(bool, String)> = self
            .workspace
            .current()
            .messages
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|m| (m.role == crate::workspace::Role::User, m.text.clone()))
            .collect();
        Frame::NONE
            .fill(theme::GLASS_SOFT)
            .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                for (is_user, text) in &msgs {
                    ui.label(
                        egui::RichText::new(if *is_user {
                            format!("你：{text}")
                        } else {
                            format!("助手：{text}")
                        })
                        .color(if *is_user {
                            theme::TEXT
                        } else {
                            theme::ACCENT_CYAN
                        }),
                    );
                }
            });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut input = self.ai_input.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut input)
                    .desired_width(420.0)
                    .hint_text("试试：确认需求 / 保存版本 / 运行实验 / 核对计划"),
            );
            if resp.changed() {
                self.ai_input = input;
            }
            if ui
                .button(egui::RichText::new("发送").color(theme::ACCENT))
                .clicked()
            {
                self.do_ai_send();
            }
        });
        if let Some(n) = &self.ai_notice {
            ui.label(egui::RichText::new(n).color(theme::TEXT_DIM));
        }

        ui.add_space(8.0);
        // 子视图切换（意图路由会自动跳转；也可手动切）
        ui.horizontal(|ui| {
            for v in AiView::ALL {
                if ui.selectable_label(self.ai_view == v, v.label()).clicked() {
                    self.ai_view = v;
                }
            }
        });
        ui.add_space(4.0);
        match self.ai_view {
            AiView::Project => self.render_ai_project(ui),
            AiView::Dev => self.render_ai_dev(ui),
            AiView::Debug => self.render_ai_debug(ui),
            AiView::PlanBridge => self.render_ai_plan_bridge(ui),
        }
        self.poll(PageKey::AiRun);
    }

    /// 对话发送：意图路由 → 预设回复 → 目标子视图跳转（Unknown 停留并诚实解释）。
    fn do_ai_send(&mut self) {
        let text = std::mem::take(&mut self.ai_input);
        if text.trim().is_empty() {
            return;
        }
        let intent = ai::route(&text);
        let reply = ai::reply(&text);
        let p = self.workspace.current_mut();
        p.messages.push(crate::workspace::ChatMessage {
            role: crate::workspace::Role::User,
            text,
        });
        p.messages.push(crate::workspace::ChatMessage {
            role: crate::workspace::Role::Assistant,
            text: reply.clone(),
        });
        if intent != Intent::Unknown {
            self.ai_view = AiView::from_intent(intent);
        }
        self.ai_notice = Some(reply);
    }

    // ---- 项目视图：项目隔离 + 需求确认（S17）
    fn render_ai_project(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("项目（会话与版本隔离；切回时消息与版本保留）")
                .color(theme::TEXT_DIM),
        );
        let names: Vec<(usize, String, bool)> = self
            .workspace
            .projects
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.name.clone(), i == self.workspace.active))
            .collect();
        ui.horizontal(|ui| {
            for (i, name, active) in names {
                if ui.selectable_label(active, name).clicked() {
                    self.workspace.switch_to(i);
                }
            }
            if ui.button("＋ 新建项目").clicked() {
                let n = self.workspace.projects.len() + 1;
                let id = self.workspace.add_project(format!("研究项目 {n}"));
                self.ai_notice = Some(format!("已新建项目 {id}：会话与版本与原项目互不串扰"));
            }
        });

        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("需求确认（生成 R 版本；确认后不可变）").color(theme::TEXT),
        );
        let mut text = self.ai_req_text.clone();
        if ui
            .add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(2)
                    .desired_width(560.0)
                    .hint_text("研究目标"),
            )
            .changed()
        {
            self.ai_req_text = text;
        }
        ui.horizontal(|ui| {
            let mut acc = self.ai_req_acceptance.clone();
            ui.label("验收标准：");
            if ui
                .add(egui::TextEdit::singleline(&mut acc).desired_width(180.0))
                .changed()
            {
                self.ai_req_acceptance = acc;
            }
            ui.label("投入比例 %：");
            ui.add(
                egui::DragValue::new(&mut self.ai_req_alloc)
                    .range(0.0..=100.0)
                    .speed(1.0),
            );
            let mut min_amt = self.ai_req_min_amount.clone();
            ui.label("最低成交额：");
            if ui
                .add(egui::TextEdit::singleline(&mut min_amt).desired_width(110.0))
                .changed()
            {
                self.ai_req_min_amount = min_amt;
            }
            if ui
                .button(egui::RichText::new("确认需求").color(theme::ACCENT))
                .clicked()
            {
                self.do_confirm_requirement();
            }
        });
        // R 版本列表（当前活动标记）
        let reqs: Vec<(usize, String)> = self
            .workspace
            .current()
            .reqs
            .iter()
            .map(|r| {
                (
                    r.id,
                    format!(
                        "R{}：{}（投入 {:.0}%，成交额 ≥{:.0}）",
                        r.id,
                        r.text,
                        r.allocation * 100.0,
                        r.min_amount
                    ),
                )
            })
            .collect();
        for (id, desc) in reqs {
            let active = self.workspace.current().active_req == Some(id);
            ui.label(
                egui::RichText::new(format!("{desc}{}", if active { "  ← 当前" } else { "" }))
                    .color(if active { theme::ACCENT } else { theme::TEXT }),
            );
        }
    }

    fn do_confirm_requirement(&mut self) {
        let min_amount = self.ai_req_min_amount.trim().parse::<f64>().unwrap_or(-1.0);
        match self.workspace.confirm_requirement(
            &self.ai_req_text,
            &self.ai_req_acceptance,
            self.ai_req_alloc,
            min_amount,
        ) {
            Ok(id) => {
                let p = self.workspace.current_mut();
                p.messages.push(crate::workspace::ChatMessage {
                    role: crate::workspace::Role::Assistant,
                    text: format!("已生成需求版本 R{id}：上游已更新，旧版本将不可新运行（历史实验保留）。"),
                });
                self.ai_notice = Some(format!("已生成需求版本 R{id}"));
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    // ---- 开发视图：设计绑定需求 + 版本不可变（S18）
    fn render_ai_dev(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("开发（设计绑定当前需求；版本不可变并冻结 R/D 引用）")
                .color(theme::TEXT_DIM),
        );
        ui.horizontal(|ui| {
            let mut note = self.ai_design_note.clone();
            ui.label("设计说明：");
            if ui
                .add(egui::TextEdit::singleline(&mut note).desired_width(240.0))
                .changed()
            {
                self.ai_design_note = note;
            }
            if ui.button("生成设计").clicked() {
                self.do_generate_design();
            }
        });
        let mut src = self.ai_code_source.clone();
        if ui
            .add(
                egui::TextEdit::multiline(&mut src)
                    .desired_rows(4)
                    .desired_width(560.0)
                    .font(FontId::monospace(12.0))
                    .hint_text("策略代码草稿"),
            )
            .changed()
        {
            self.ai_code_source = src;
        }
        if ui
            .button(egui::RichText::new("保存版本（不可变）").color(theme::ACCENT))
            .clicked()
        {
            self.do_save_version();
        }
        // 版本列表：冻结引用 + 新鲜状态
        let rows: Vec<(usize, usize, usize, bool, bool)> = {
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
        for (id, req, design, fresh, active) in rows {
            ui.label(
                egui::RichText::new(format!(
                    "v{id}（R{req}/D{design}）{}{}",
                    if fresh { " 新鲜" } else { " 已过期" },
                    if active { "  ← 当前" } else { "" }
                ))
                .color(if fresh { theme::ACCENT } else { theme::TEXT_DIM }),
            );
        }
    }

    fn do_generate_design(&mut self) {
        let note = std::mem::take(&mut self.ai_design_note);
        match self.workspace.generate_design(note) {
            Ok(id) => self.ai_notice = Some(format!("已生成设计 D{id}，绑定当前需求版本")),
            Err(e) => self.ai_notice = Some(e),
        }
    }

    fn do_save_version(&mut self) {
        let src = std::mem::take(&mut self.ai_code_source);
        match self.workspace.save_version(src) {
            Ok(id) => {
                self.ai_notice =
                    Some(format!("已保存版本 v{id}：冻结 R/D 引用与输入，不可变"))
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    // ---- 调试视图：实验真实执行（协调器）+ 历史保留（S18/S19）
    fn render_ai_debug(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("调试（实验冻结当前版本执行；上游过期即拒绝；历史实验不变）")
                .color(theme::TEXT_DIM),
        );
        let (vid, fresh) = {
            let p = self.workspace.current();
            (
                p.active_version,
                p.active_version
                    .map(|v| self.workspace.version_fresh(v))
                    .unwrap_or(false),
            )
        };
        match vid {
            Some(v) => ui.label(egui::RichText::new(format!(
                "当前版本 v{v}：{}",
                if fresh { "可运行" } else { "已过期，请重新保存版本" }
            ))
            .color(if fresh { theme::ACCENT } else { theme::DANGER })),
            None => ui.label(egui::RichText::new("尚无版本：请先在开发页保存").color(theme::DANGER)),
        };
        ui.horizontal(|ui| {
            if ui
                .button(egui::RichText::new("运行实验").color(theme::ACCENT))
                .clicked()
            {
                self.do_ai_run();
            }
            if self.ai_run.watch.is_active() {
                if let Some(t) = self.ai_run.watch.task_id() {
                    let t = t.to_string();
                    if ui
                        .button(egui::RichText::new("取消").color(theme::DANGER))
                        .clicked()
                    {
                        self.do_cancel(t, PageKey::AiRun);
                    }
                }
            }
        });
        render_watch(ui, &self.ai_run);
        // 实验列表（含历史；取消无产物时收益列为「—」）
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
        if !exps.is_empty() {
            ui.add_space(8.0);
            egui::Grid::new("ai-exp-grid").show(ui, |ui| {
                for head in ["实验", "版本", "任务", "期末收益"] {
                    ui.label(egui::RichText::new(head).color(theme::TEXT_DIM));
                }
                ui.end_row();
                for (id, ver, task, ret) in exps {
                    ui.label(format!("E{id}"));
                    ui.label(format!("v{ver}"));
                    ui.label(
                        egui::RichText::new(task.as_deref().unwrap_or("（无任务）"))
                            .font(FontId::monospace(11.0)),
                    );
                    ui.label(ret.unwrap_or_else(|| "—".into()));
                    ui.end_row();
                }
            });
        }
    }

    /// 运行实验：先在工作台冻结（过期在此拒绝），再提交真实协调器运行。
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
                .on_submit_failed("尚无数据快照：请先在数据快照页导入");
            return;
        };
        let Some(u) = &self.universe_saved else {
            self.ai_run
                .on_submit_failed("尚无股票池版本：请先在股票池页保存");
            return;
        };
        let universe_id = u.universe_id.clone();
        let form = self.run_form.clone();
        let Some(bridge) = &self.bridge else { return };
        match bridge.submit_run(&snapshot_id, &universe_id, &form) {
            Ok(r) => {
                let task_id = r.task_id.clone();
                self.ai_run.on_submitted(r.task_id, "AI 工作台实验运行中…");
                self.workspace.attach_task(exp_id, task_id);
            }
            Err(e) => self
                .ai_run
                .on_submit_failed(format!("运行被拒绝{}", crate::pipeline::rejection_text(&e))),
        }
    }

    // ---- 计划桥视图：冻结签名 + 核对门控 + 确认导出（S20）
    fn render_ai_plan_bridge(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("计划桥（计划冻结签名；仅核对通过且确认后可导出 CSV）")
                .color(theme::TEXT_DIM),
        );
        ui.horizontal(|ui| {
            let mut d = self.ai_plan_trade_date.clone();
            ui.label("交易日：");
            if ui
                .add(egui::TextEdit::singleline(&mut d).desired_width(120.0))
                .changed()
            {
                self.ai_plan_trade_date = d;
            }
            ui.label(
                egui::RichText::new("（数据快照取最新已导入）").color(theme::TEXT_DIM),
            );
        });
        let mut json = self.ai_plan_json.clone();
        if ui
            .add(
                egui::TextEdit::multiline(&mut json)
                    .desired_rows(4)
                    .desired_width(560.0)
                    .font(FontId::monospace(12.0))
                    .hint_text(r#"{"cash_cny":"10000","total_assets_cny":"20000","positions":[…]}"#),
            )
            .changed()
        {
            self.ai_plan_json = json;
        }
        ui.horizontal(|ui| {
            if ui.button("生成计划").clicked() {
                self.do_ai_plan_generate();
            }
            if ui.button("核对").clicked() {
                self.do_ai_plan_check();
            }
            if ui
                .button(egui::RichText::new("确认导出").color(theme::ACCENT))
                .clicked()
            {
                self.do_ai_plan_export();
            }
        });
        if let Some(issues) = &self.ai_plan_issues {
            if issues.is_empty() {
                ui.label(egui::RichText::new("核对通过：可确认导出").color(theme::ACCENT));
            } else {
                for i in issues {
                    ui.label(egui::RichText::new(format!("⚠ {i}")).color(theme::DANGER));
                }
            }
        }
        if let Some(csv) = &self.ai_plan_csv {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("导出内容（含演示标识）：").color(theme::TEXT_DIM));
            ui.label(egui::RichText::new(csv).font(FontId::monospace(11.0)).color(theme::TEXT));
        }
    }

    fn do_ai_plan_generate(&mut self) {
        let Some(snapshot_id) = self.pick_snapshot().map(|(_, id)| id) else {
            self.ai_notice = Some("尚无数据快照：请先在数据快照页导入".into());
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
                self.ai_notice = Some("已生成计划草稿：账户与数据输入已冻结为签名".into());
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    fn do_ai_plan_check(&mut self) {
        // 买入含费合计：Σ 目标买入金额 ×（1 + 佣金率）——费率取运行页成本假设
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
                self.ai_notice = Some(if issues.is_empty() {
                    "核对通过".into()
                } else {
                    format!("核对未通过：{} 项问题", issues.len())
                });
                self.ai_plan_issues = Some(issues);
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
                    Ok(()) => self.ai_notice = Some(format!("已确认导出：{}", path.display())),
                    Err(e) => {
                        self.ai_notice = Some(format!("文件写入失败：{e}（CSV 内容保留在下方）"))
                    }
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

/// 页面任务观察的通用渲染（状态行 / 错误行）。
fn render_watch(ui: &mut egui::Ui, page: &PageState<TaskWatch>) {
    if let Some(err) = &page.error {
        ui.label(egui::RichText::new(&err.text).color(theme::DANGER));
    }
    match &page.watch {
        TaskWatch::Idle => {}
        TaskWatch::Submitted { label, .. } => {
            ui.label(egui::RichText::new(*label).color(theme::ACCENT_CYAN));
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
            ui.label(
                egui::RichText::new(format!("任务 {}：{state}", v.task_id))
                    .color(if ok { theme::ACCENT } else { theme::DANGER }),
            );
        }
    }
}

fn fmt_metric(m: &nautilus_research_domain::metrics::NullableMetric) -> String {
    m.value.clone().unwrap_or_else(|| "—".into())
}
