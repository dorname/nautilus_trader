//! eframe App：六页导航 + 玻璃态面板 + 按需重绘 + 流水线五页协调器对接。
//!
//! 按需重绘约束（验收红线：不能打爆 CPU）：静默时不请求重绘（零帧）；
//! 仅当存在活跃任务时以 `request_repaint_after(POLL_INTERVAL)` 定时轮询
//! `get_task`（轻量只读），任务推进到终态立即落定并停止轮询。

use egui::{Color32, CornerRadius, FontId, Frame, Margin, Pos2, Stroke};

use crate::bridge::{CompareForm, DesktopBridge, ImportForm, PlanForm, RunForm, UniverseForm};
use crate::layout;
use crate::nav::{Page, ALL_PAGES};
use crate::pipeline::{terminal_error_text, PageState, TaskWatch, POLL_INTERVAL};
use crate::session::Session;
use crate::theme;

/// 提交类页面的轮询键。
#[derive(Debug, Clone, Copy, PartialEq)]
enum PageKey {
    Snapshot,
    Universe,
    Run,
    Plan,
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
    /// 活跃任务计数（四页提交类任务）。
    fn active_task_count(&self) -> usize {
        [
            &self.snapshot_page.watch,
            &self.universe_page.watch,
            &self.run_page.watch,
            &self.plan_page.watch,
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
        }
    }

    /// 页面状态写回。
    fn set_page_state(&mut self, key: PageKey, state: PageState<TaskWatch>) {
        match key {
            PageKey::Snapshot => self.snapshot_page = state,
            PageKey::Universe => self.universe_page = state,
            PageKey::Run => self.run_page = state,
            PageKey::Plan => self.plan_page = state,
        }
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
            Page::AiWorkspace => {
                ui.heading(egui::RichText::new("AI 工作台").color(theme::TEXT));
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(Page::AiWorkspace.placeholder())
                        .color(theme::TEXT_DIM),
                );
            }
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
                            self.do_cancel(task_id);
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

    fn do_cancel(&mut self, task_id: String) {
        let Some(bridge) = &self.bridge else { return };
        match bridge.cancel(&task_id) {
            Ok(_) => {}
            Err(e) => self
                .run_page
                .note_terminal_error(format!("取消失败{}", crate::pipeline::rejection_text(&e))),
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
