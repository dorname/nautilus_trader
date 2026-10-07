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

/// 原型 `plan()` 参考快照选项。
const PLAN_SNAP_IDS: [&str; 2] = ["PLAN-20260108", "PLAN-20260105"];
const PLAN_SNAP_LABELS: [&str; 2] = ["01-08 收盘 · 合成计划快照", "01-05 收盘 · 过期示例"];
const PLAN_SYMS: [&str; 3] = ["SYN-A", "SYN-B", "SYN-C"];
const PLAN_PRICES: [f64; 3] = [11.00, 19.50, 8.50];

/// 提交类页面的轮询键。
#[derive(Debug, Clone, Copy, PartialEq)]
enum PageKey {
    Snapshot,
    Universe,
    Plan,
    /// 实验运行（事件调试 / 回测实验共用入口）。
    Run,
}

/// 离线演示任务动作（对齐原型 `startTask`，约 850ms 后提交）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DemoAction {
    /// 从需求生成设计草稿（不保存 D）。
    DesignDraft,
    /// 生成策略代码草稿。
    GenerateCode,
    /// 模拟更新数据快照。
    UpdateData,
}

/// 进行中的离线演示任务。
#[derive(Debug, Clone)]
struct DemoTask {
    /// 任务序号（取消时与当前任务比对，避免过期完成）。
    id: u64,
    label: &'static str,
    action: DemoAction,
    /// 启动时的项目修订计数（输入变化则丢弃结果）。
    stamp: u64,
    started: std::time::Instant,
}

/// 净值比较面板缓存（RD-005：键 = 最近两次实验任务 ID 对，（次新, 最新）；
/// 实验集合或任务终态变化后失效重取，避免每帧读库）。
#[derive(Debug, Default, Clone)]
pub struct EquityChartState {
    /// 最近一次拉取对应的（次新任务, 最新任务）。
    pub key: (Option<String>, Option<String>),
    /// （系列标签, 点列），旧→新顺序（最新在最后，对齐原型着色）。
    pub series: Vec<(String, Vec<crate::bridge::EquityPoint>)>,
    /// 拉取错误（诚实占位，不空白冒充已实现）。
    pub error: Option<String>,
    /// 存在运行未完成的实验（曲线待终态，显式提示而非报错）。
    pub pending: bool,
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

    // ---- 股票池（演示层筛选 + 协调器规则）
    pub universe_form: UniverseForm,
    pub universe_page: PageState<TaskWatch>,
    /// 股票池搜索关键字（原型 `poolSearch`）。
    pub pool_search: String,
    /// 股票池市场筛选索引：0=全部 / 1=沪市 / 2=深市。
    pub pool_market: usize,
    /// 候选样本详情弹层（fixture 下标）。
    pub pool_stock_detail: Option<usize>,

    // ---- 回测实验（运行参数 + 协调器比较）
    pub run_form: RunForm,
    pub compare_form: CompareForm,
    pub compare_error: Option<String>,
    pub compare_result: Option<nautilus_research_domain::protocol::Comparison>,
    /// 净值比较面板缓存（RD-005）。
    pub equity_chart: EquityChartState,

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
    // 策略设计：设计说明与投入/成交额草稿（原型 designAllocation / designAmount，万元）
    pub ai_design_note: String,
    pub ai_design_alloc: f64,
    pub ai_design_min_amount: String,
    pub ai_code_source: String,
    /// 上次「检查草稿」通过时的源码快照（原型 `checked`；与当前草稿相等才可保存）。
    pub ai_code_checked: Option<String>,
    /// 展示资金约束修复差异（原型 `showDiff`）。
    pub show_code_diff: bool,
    // 实验任务与当前实验引用
    pub ai_run: PageState<TaskWatch>,
    pub ai_experiment: Option<usize>,
    // 计划桥：交易日/持仓 JSON/核对结果/导出产物
    pub ai_plan_trade_date: String,
    pub ai_plan_json: String,
    /// 参考快照下标（0 = PLAN-20260108，1 = 过期示例）。
    pub ai_plan_snapshot: usize,
    pub ai_plan_cash: String,
    pub ai_plan_qty: [String; 3],
    pub ai_plan_sell: [String; 3],
    pub ai_plan_issues: Option<Vec<String>>,
    pub ai_plan_csv: Option<String>,

    // ---- 呈现层状态（本批新增）
    /// 专注模式（收起右对话栏）。
    pub focus: bool,
    /// 运行记录浮层开关。
    pub show_logs: bool,
    /// 运行记录（任务动作、依据和产物；不含模型内部推理）。
    pub logs: Vec<String>,
    /// 离线演示任务（设计生成 / 代码生成 / 数据更新）。
    demo_task: Option<DemoTask>,
    /// 可重试的上次离线任务。
    demo_retry: Option<( &'static str, DemoAction)>,
    /// 下次离线任务模拟失败（运行记录控制）。
    fail_next: bool,
    demo_task_seq: u64,
    /// 对话产物卡（项目 ID, 消息序号 → 目标路由 + 原型 title/desc）。
    pub chat_cards: Vec<ArtifactCard>,
    /// 新建项目浮层。
    pub new_project_open: bool,
    pub new_project_name: String,
    /// 设计页处理图 tab：true = 流程图，false = 时序图。
    pub design_tab_flow: bool,
    /// 设计页选中节点（NODES 下标）。
    pub design_node: usize,
    /// 调试页节点筛选：空 = 全部事件。
    pub debug_filter: String,
    /// 调试页比较的两个实验 ID。
    pub debug_cmp: [usize; 2],
    /// 版本只读查看浮层（版本 ID, 源码, 可选高亮行号）。
    pub view_source: Option<(usize, String, Option<u32>)>,
    /// 需求只读查看浮层（需求版本 ID）。
    pub view_req: Option<usize>,
    /// 调试页当前选中的实验 ID。
    pub debug_run_id: usize,
    /// 调试页当前事件序号（1-based；0 = 起点前）。
    pub debug_event_index: usize,
    /// 调试页是否在自动播放。
    pub debug_playing: bool,
    /// 开发页待定位的源码行（1-based）。
    pub jump_code_line: Option<u32>,
    /// 计划导出二次确认浮层。
    pub plan_export_confirm: bool,
    /// 调试自动播放节拍。
    debug_play_at: Option<std::time::Instant>,
    /// 对话区回底标记（新消息后滚动到底部）。
    chat_pin: bool,
    /// 上一帧路由（用于一次性恢复滚动位置）。
    last_route: Option<Route>,
}

/// 对话产物卡（对齐原型 `.artifact-card`：图标 + 标题 + 说明 + 箭头）。
#[derive(Debug, Clone)]
pub struct ArtifactCard {
    pub project_id: usize,
    pub msg_index: usize,
    pub route: Route,
    pub title: String,
    pub desc: String,
}

impl ResearchApp {
    /// 新应用（参数 `font_missing` 已废弃：字体经内嵌 Noto Sans SC 兜底；
    /// 协调器桥由 `attach_bridge` 建立）。
    pub fn new(_font_missing: bool) -> Self {
        Self {
            session: Session::new(),
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
            pool_search: String::new(),
            pool_market: 0,
            pool_stock_detail: None,
            run_form: RunForm::default(),
            compare_form: CompareForm::default(),
            compare_error: None,
            compare_result: None,
            equity_chart: EquityChartState::default(),
            plan_form: PlanForm::default(),
            plan_page: PageState::default(),
            plan_exported: None,
            workspace: Workspace::new(),
            ai_input: String::new(),
            ai_notice: None,
            ai_req_text: crate::workspace::DRAFT_REQ_TEXT.into(),
            ai_req_acceptance: crate::workspace::DRAFT_ACCEPTANCE.into(),
            ai_req_alloc: crate::workspace::DRAFT_ALLOC_PCT,
            ai_req_min_amount: crate::workspace::DRAFT_MIN_AMOUNT_WAN.into(),
            ai_design_note: crate::workspace::DRAFT_DESIGN_NOTE.into(),
            ai_design_alloc: crate::workspace::DRAFT_ALLOC_PCT,
            ai_design_min_amount: crate::workspace::DRAFT_MIN_AMOUNT_WAN.into(),
            ai_code_source: crate::workspace::DRAFT_CODE_ORIGINAL.into(),
            ai_code_checked: None,
            show_code_diff: false,
            ai_run: PageState::default(),
            ai_experiment: None,
            ai_plan_trade_date: "2026-01-09".into(),
            ai_plan_json: String::new(),
            ai_plan_snapshot: 0,
            ai_plan_cash: "10000".into(),
            ai_plan_qty: ["0".into(), "0".into(), "0".into()],
            ai_plan_sell: ["0".into(), "0".into(), "0".into()],
            ai_plan_issues: None,
            ai_plan_csv: None,
            focus: false,
            show_logs: false,
            logs: Vec::new(),
            demo_task: None,
            demo_retry: None,
            fail_next: false,
            demo_task_seq: 0,
            // 欢迎消息挂需求草稿产物卡（与原型 createProject 首条一致）
            chat_cards: vec![ArtifactCard {
                project_id: 1,
                msg_index: 0,
                route: Route::Requirements,
                title: crate::nav::WELCOME_CARD_TITLE.into(),
                desc: crate::nav::WELCOME_CARD_DESC.into(),
            }],
            new_project_open: false,
            new_project_name: String::new(),
            design_tab_flow: true,
            design_node: 3,
            debug_cmp: [1, 2],
            debug_filter: String::new(),
            view_source: None,
            view_req: None,
            debug_run_id: 0,
            debug_event_index: 0,
            debug_playing: false,
            jump_code_line: None,
            plan_export_confirm: false,
            debug_play_at: None,
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_root(ui);
    }
}

// ================================================================ 布局骨架

impl ResearchApp {
    /// 根 Ui 渲染（eframe 0.36 模型）：环境柔光 + 左侧栏卡 + 主区卡（顶栏/画布+对话/footer）。
    /// RD-008：提取为独立方法，供 ST-S15-06 无头几何回归经 egui::Context::run_ui 驱动
    /// （pub 仅为测试可达，GUI 入口仍是 eframe::App::ui）。
    #[doc(hidden)]
    pub fn render_root(&mut self, ui: &mut egui::Ui) {
        // 仅 Ctrl/Cmd + 滚轮缩放：egui 会把该手势写入 zoom_delta 并清零
        // smooth_scroll_delta。纯滚轮 / 触控板双指滑动不得改 zoom_factor
        // （部分环境会额外发出 Pinch→Event::Zoom，必须用修饰键门禁过滤）。
        // 键盘 Ctrl+/−/0 由 egui Options::zoom_with_keyboard 默认处理。
        let (zd, zoom_chord) = ui.ctx().input(|i| {
            let chord = i.modifiers.command || i.modifiers.ctrl;
            (i.zoom_delta(), chord)
        });
        if zoom_chord && (zd - 1.0).abs() > 0.001 {
            let next = (ui.ctx().zoom_factor() * zd).clamp(0.5, 2.0);
            ui.ctx().set_zoom_factor(next);
        }
        // 字号倍率固定 1.0：缩放统一交给 zoom_factor，避免双重缩放
        theme::set_type_scale(1.0);
        ui.spacing_mut().button_padding = egui::vec2(12.0, 8.0);

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
        // 根三列显式切分（desktop-root-layout）：horizontal + Frame 自适应在
        // egui 0.36 下会塌缩成内容固有宽——改为矩形切列、各列独立 top_down Ui
        let (sidebar_w, main_w) = layout::columns(outer.width(), plan);
        let sidebar_rect =
            egui::Rect::from_min_size(outer.min, egui::vec2(sidebar_w, outer.height()));
        let main_rect = egui::Rect::from_min_size(
            outer.min + egui::vec2(sidebar_w + layout::APP_GAP, 0.0),
            egui::vec2(main_w, outer.height()),
        );
        let mut sidebar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(sidebar_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        self.render_sidebar(&mut sidebar_ui, plan);
        let mut main_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(main_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        self.render_main(&mut main_ui, plan);

        // 轮询所有提交类任务：get_task → 状态机推进（终态落定并停止轮询）
        for key in [
            PageKey::Snapshot,
            PageKey::Universe,
            PageKey::Plan,
            PageKey::Run,
        ] {
            self.poll(key);
        }
        self.poll_demo_task();
        // 调试自动播放：约 700ms 推进一事件
        if self.debug_playing {
            let due = self
                .debug_play_at
                .map(|t| t.elapsed() >= std::time::Duration::from_millis(700))
                .unwrap_or(true);
            if due {
                self.debug_play_tick();
                self.debug_play_at = Some(std::time::Instant::now());
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        // 按需重绘：有活跃任务时 ~20fps 驱动 spinner（帧内仍 poll）；否则事件驱动
        if self.active_task_count() > 0 || self.demo_task.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(50).max(POLL_INTERVAL / 10));
        }
    }

    /// 自动播放推进一格（`debug_event_index` = 筛选列表 0-based 下标）。
    fn debug_play_tick(&mut self) {
        let n = self.debug_filtered_len();
        if n == 0 {
            self.debug_playing = false;
            return;
        }
        if self.debug_event_index + 1 >= n {
            self.debug_event_index = n - 1;
            self.debug_playing = false;
            return;
        }
        self.debug_event_index += 1;
    }

    fn debug_filtered_len(&self) -> usize {
        self.workspace
            .current()
            .experiments
            .iter()
            .find(|e| e.id == self.debug_run_id)
            .and_then(|e| e.demo.as_ref())
            .map(|d| {
                d.events
                    .iter()
                    .filter(|ev| self.debug_filter.is_empty() || ev.node == self.debug_filter)
                    .count()
            })
            .unwrap_or(0)
    }

    /// 左侧栏（唯一导航）：品牌 + 项目选择/新建 + 两组 11 路由 + 底部离线声明。
    fn render_sidebar(&mut self, ui: &mut egui::Ui, plan: LayoutPlan) {
        // 列 Ui 已带固定 max_rect：玻璃卡直接矩形绘制，内容在带内边距的子列内排布
        let card = ui.max_rect();
        theme::paint_drop_shadow(ui.painter(), card, 16.0);
        ui.painter().rect(
            card,
            CornerRadius::same(16),
            theme::GLASS,
            Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );
        theme::paint_inset_top(ui.painter(), card, 14.0);
        let content = card.shrink2(egui::vec2(14.0, 0.0));
        let content = egui::Rect::from_min_max(
            Pos2::new(content.left(), card.top() + 22.0),
            Pos2::new(content.right(), card.bottom() - 16.0),
        );
        let mut cui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(Layout::top_down(Align::Min)),
        );
        let ui = &mut cui;
        {
            let slim = plan == LayoutPlan::Slim;
            let w = if slim {
                layout::SIDEBAR_W_SLIM - 16.0
            } else {
                layout::sidebar_w(plan) - 28.0
            };
            // 品牌：研 + 研序 / 策略研究工作区（原型 .brand-mark：145° 渐变 + 辉光；
            // Slim 档隐藏品牌名，只留渐变 mark 居中）
            if slim {
                let (mark_rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(29.0), Sense::hover());
                theme::paint_brand_mark(ui.painter(), mark_rect);
            } else {
                ui.horizontal(|ui| {
                    let (mark_rect, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(29.0), Sense::hover());
                    theme::paint_brand_mark(ui.painter(), mark_rect);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        // 原型 `.brand`：letter-spacing 3px
                        ui.label(
                            egui::RichText::new("研序")
                                .font(FontId::proportional(theme::fs(20.0)))
                                .extra_letter_spacing(3.0)
                                .color(theme::TEXT),
                        );
                        ui.label(lbl("策略研究工作区", 9.0, theme::MUTED));
                    });
                });
            }
            ui.add_space(18.0);

            // 项目选择 + 新建（Slim 档隐藏文字控件）
            if !slim {
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
                    .add_sized([w, 24.0], theme::small_ghost_button("＋ 新建研究项目"))
                    .clicked()
                {
                    self.new_project_open = true;
                }
                ui.add_space(18.0);
            }

            // 研究工作区 8 路由
            if !slim {
                theme::section_label(ui, WORKSPACE_GROUP);
            }
            for r in WORKSPACE_ROUTES {
                self.nav_item(ui, r, w, slim);
            }
            ui.add_space(14.0);
            // 研究资源 3 路由
            if !slim {
                theme::section_label(ui, RESOURCE_GROUP);
            }
            for r in RESOURCE_ROUTES {
                self.nav_item(ui, r, w, slim);
            }

            // 底部：本地声明（Slim 档只留头像）
            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                ui.separator();
                ui.add_space(10.0);
                // RD-008 同类：bottom_up 内 horizontal 按初始行高取 frame，Frame 继承
                // 布局使头像文本居中整行、后续 vertical 子列被挤出右缘——改显式占位 +
                // 矩形定位：头像居左，双行声明放 top_down 子列
                let (row, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 34.0),
                    Sense::hover(),
                );
                let avatar_rect = egui::Rect::from_min_size(
                    row.min + egui::vec2(0.0, 3.5),
                    egui::vec2(27.0, 27.0),
                );
                ui.painter().rect(
                    avatar_rect,
                    CornerRadius::same(14),
                    theme::GLASS_STRONG,
                    Stroke::new(1.0, theme::BORDER_STRONG),
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    avatar_rect.center(),
                    ALIGN2_CENTER,
                    "本地",
                    FontId::proportional(theme::fs(10.0)),
                    theme::TEXT2,
                );
                if !slim {
                    let mut text_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(egui::Rect::from_min_size(
                                row.min + egui::vec2(33.0, 4.0),
                                egui::vec2((row.width() - 33.0).max(0.0), 26.0),
                            ))
                            .layout(Layout::top_down(Align::Min)),
                    );
                    text_ui.label(lbl("个人研究空间", 11.0, theme::TEXT2));
                    text_ui.label(lbl("离线 · 无自动下单", 9.0, theme::MUTED));
                }
            });
        }
    }

    /// 导航项：激活 = 绿柔底 + 主色描边 + 左缘 2px 指示条；回测实验附实验计数。
    /// Slim 档（≤900）只显居中图标、隐藏文字与计数（原型 @900 图标栏）。
    fn nav_item(&mut self, ui: &mut egui::Ui, route: Route, w: f32, slim: bool) {
        let active = self.session.route == route;
        // 原型 nav：回测实验始终显示 runs.length（含 0）
        let count = (route == Route::Experiments && !slim)
            .then(|| self.workspace.current().experiments.len());
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, theme::fs(30.0)), Sense::click());
        let fill = if active {
            theme::ACCENT_SOFT
        } else if resp.hovered() {
            Color32::from_rgba_premultiplied(10, 10, 10, 10)
        } else {
            Color32::TRANSPARENT
        };
        let stroke = if active {
            Stroke::new(1.0, theme::ACCENT_BORDER)
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
        let icon_cx = if slim {
            rect.center().x
        } else {
            rect.left() + 11.0 + 8.0
        };
        let icon_rect = egui::Rect::from_center_size(
            Pos2::new(icon_cx, rect.center().y),
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
        let cy = rect.center().y;
        if !slim {
            let tx = rect.left() + 11.0 + 24.0;
            ui.painter().text(
                Pos2::new(tx, cy),
                egui::Align2::LEFT_CENTER,
                route.title(),
                FontId::proportional(theme::fs(12.0)),
                if active { theme::TEXT } else { theme::MUTED },
            );
            if let Some(n) = count {
                ui.painter().text(
                    Pos2::new(rect.right() - 10.0, cy),
                    egui::Align2::RIGHT_CENTER,
                    n.to_string(),
                    FontId::monospace(theme::fs(10.0)),
                    theme::FAINT,
                );
            }
        }
        if active {
            // 左缘 2px 绿指示条 + 辉光（原型 .active:before + box-shadow 0 0 10px accent-glow）：
            // 三层透明度递减的同心条近似高斯辉光
            for (half, alpha) in [(9.0_f32, 115u8), (13.0, 61), (17.0, 26)] {
                ui.painter().line_segment(
                    [
                        Pos2::new(rect.left(), cy - half),
                        Pos2::new(rect.left(), cy + half),
                    ],
                    Stroke::new(4.0, Color32::from_rgba_premultiplied(34, 197, 94, alpha)),
                );
            }
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
        // 列 Ui 已带固定 max_rect：用 max_rect 高度做三段切分（避免依赖
        // available_height 在 Frame 包裹语义下的行高塌缩）
        let card = ui.max_rect();
        theme::paint_drop_shadow(ui.painter(), card, 16.0);
        ui.painter().rect(
            card,
            CornerRadius::same(16),
            theme::MAIN_BG,
            Stroke::new(1.0, theme::BORDER),
            egui::StrokeKind::Inside,
        );
        theme::paint_inset_top(ui.painter(), card, 14.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        let (_, body_h, _) = layout::rows(card.height());
        self.render_topbar(ui, card.width());
        self.render_body(ui, card.width(), body_h, plan);
        self.render_footer(ui, card.width());

        // 浮层：运行记录 / 版本查看 / 新建项目
        self.render_windows(ui.ctx());
    }

    /// 顶栏：面包屑（研究项目 / 项目名）+ 演示环境 tag + 运行记录按钮。
    /// 显式左右切带，避免 `horizontal` + `right_to_left` 把右簇换行叠进对话栏。
    fn render_topbar(&mut self, ui: &mut egui::Ui, w: f32) {
        let project = self.workspace.current().name.clone();
        let (rect, _) = ui.allocate_exact_size(sz(w, layout::TOPBAR_H), Sense::hover());
        // 原型 `.topbar`：rgba(255,255,255,.02) 衬底 + 底部分割
        ui.painter().rect_filled(rect, CornerRadius::ZERO, theme::WHITE_03);
        ui.painter().line_segment(
            [
                Pos2::new(rect.left(), rect.bottom()),
                Pos2::new(rect.right(), rect.bottom()),
            ],
            Stroke::new(1.0, theme::BORDER),
        );
        let mid = rect.center().x;
        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    Pos2::new(rect.left() + 25.0, rect.top()),
                    Pos2::new(mid - 8.0, rect.bottom()),
                ))
                .layout(egui::Layout::left_to_right(Align::Center)),
        );
        left.label(lbl("研究项目", 12.0, theme::MUTED));
        left.add_space(10.0);
        left.label(lbl("/", 12.0, theme::FAINT));
        left.add_space(10.0);
        left.label(lbl(project, 12.0, theme::TEXT2));
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    Pos2::new(mid, rect.top()),
                    Pos2::new(rect.right() - 25.0, rect.bottom()),
                ))
                .layout(Layout::right_to_left(Align::Center)),
        );
        if right.add(theme::small_ghost_button("运行记录")).clicked() {
            self.show_logs = true;
        }
        right.add_space(8.0);
        theme::tag_live_ui(&mut right, "演示环境");
    }

    /// footer：本地研究桌面声明 + 溯源声明（RD-006：移除「原型/刷新重置」HTML 语义，
    /// 原生桌面工作区 SQLite 持久化、无浏览器刷新概念）。
    fn render_footer(&mut self, ui: &mut egui::Ui, w: f32) {
        let (rect, _) = ui.allocate_exact_size(sz(w, layout::FOOTER_H), Sense::hover());
        // 原型 `.footer`：rgba(0,0,0,.25) 衬底
        ui.painter().rect_filled(
            rect,
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(0, 0, 0, 64),
        );
        ui.painter().line_segment(
            [Pos2::new(rect.left(), rect.top()), Pos2::new(rect.right(), rect.top())],
            Stroke::new(1.0, theme::BORDER),
        );
        let mut left = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    Pos2::new(rect.left() + 20.0, rect.top()),
                    Pos2::new(rect.center().x, rect.bottom()),
                ))
                .layout(Layout::left_to_right(Align::Center)),
        );
        left.label(lbl(theme::FOOTER_LEFT, 9.0, theme::FAINT));
        let mut right = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    Pos2::new(rect.center().x, rect.top()),
                    Pos2::new(rect.right() - 20.0, rect.bottom()),
                ))
                .layout(Layout::right_to_left(Align::Center)),
        );
        right.label(lbl("所有结果可追溯至输入与版本", 9.0, theme::FAINT));
    }

    /// 主体：工作区画布（含 workspace-head）+ 右对话栏（专注模式收起）。
    /// RD-008：显式矩形切列 + new_child 建独立 top_down 列 Ui（沿用 desktop-root-layout
    /// 根区修复模式）——此前用 ui.horizontal + allocate_ui，子 Ui 继承
    /// left_to_right(Center) 布局：workspace-head 垂直居中、页面内容在零宽区一字一行、
    /// 对话内容溢出窗外（egui allocate_ui 继承父布局语义）。
    fn render_body(&mut self, ui: &mut egui::Ui, w: f32, h: f32, plan: LayoutPlan) {
        // 显式占位：new_child 不消耗父级游标，须先按 (w, h) 推进（否则 footer 上移）
        let (body, _) = ui.allocate_exact_size(sz(w, h), egui::Sense::hover());
        let chat_w = if self.focus {
            0.0
        } else {
            layout::chat_w(plan)
        };
        let canvas_rect = egui::Rect::from_min_size(body.min, egui::vec2(w - chat_w, h));
        let mut canvas_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(canvas_rect)
                .layout(Layout::top_down(Align::Min)),
        );
        self.render_canvas(&mut canvas_ui, w - chat_w, h);
        if !self.focus {
            let chat_rect = egui::Rect::from_min_size(
                body.min + egui::vec2(w - chat_w, 0.0),
                egui::vec2(chat_w, h),
            );
            ui.painter()
                .rect_filled(chat_rect, CornerRadius::ZERO, theme::CHAT_BG);
            let mut chat_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(chat_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            self.render_chat(&mut chat_ui, chat_w, h);
        }
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
                        let (mark, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(12.0), Sense::hover());
                        icons::paint_diamond(ui.painter(), mark, theme::ACCENT_TEXT);
                        ui.add_space(8.0);
                        ui.label(lbl("项目产物", 12.0, theme::TEXT2));
                        ui.add_space(8.0);
                        theme::tag_ui(ui, &version_tag, TagKind::Neutral);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(23.0);
                            // 内嵌字体无 ⤢ → 空框；矢量图标 + 文案均可点
                            let label = if self.focus {
                                "返回对话"
                            } else {
                                "展开工作区"
                            };
                            let (ir, icon_resp) =
                                ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::click());
                            icons::paint_expand(ui.painter(), ir, theme::TEXT2);
                            ui.add_space(4.0);
                            let text_resp = ui.add(theme::small_ghost_button(label));
                            if text_resp.clicked() || icon_resp.clicked() {
                                self.focus = !self.focus;
                            }
                        });
                    });
                })
                .response;
            let _ = head;

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
                ui.set_max_width(w);
                ui.add_space(20.0);
                let pad = 25.0;
                // 专注模式：原型 `.focus .canvas { max-width:1120px; margin:0 auto }`
                let band_w = if self.focus {
                    w.min(layout::FOCUS_CANVAS_MAX_W)
                } else {
                    w
                };
                let side = ((w - band_w) * 0.5).max(0.0);
                let inner_w = (band_w - pad * 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(side + pad);
                    ui.allocate_ui_with_layout(
                        sz(inner_w, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_max_width(inner_w);
                            self.render_notice(ui);
                            self.render_page(ui, route);
                            ui.add_space(24.0);
                        },
                    );
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

            // 头部：28×28 助手徽标 + 标题 + 预设对话（原型无底部分割线）
            let (head, _) = ui.allocate_exact_size(sz(w, 64.0), Sense::hover());
            let logo = egui::Rect::from_center_size(
                Pos2::new(head.left() + 36.0, head.center().y),
                egui::Vec2::splat(28.0),
            );
            ui.painter().rect(
                logo,
                CornerRadius::same(8),
                theme::ACCENT_SOFT,
                Stroke::new(1.0, theme::ACCENT_BORDER),
                egui::StrokeKind::Inside,
            );
            icons::paint_sparkle(ui.painter(), logo.shrink(6.0), theme::ACCENT_TEXT);
            let mut title_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_max(
                        Pos2::new(logo.right() + 8.0, head.top() + 12.0),
                        Pos2::new(head.right() - 92.0, head.bottom() - 8.0),
                    ))
                    .layout(Layout::top_down(Align::Min)),
            );
            title_ui.label(lbl("研究助手", 13.0, theme::TEXT));
            title_ui.label(lbl("从一个想法，到可追溯的计划", 9.0, theme::MUTED));
            let mut tag_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_center_size(
                        Pos2::new(head.right() - 48.0, head.center().y),
                        egui::vec2(72.0, 22.0),
                    ))
                    .layout(egui::Layout::left_to_right(Align::Center)),
            );
            theme::tag_ui(&mut tag_ui, "预设对话", TagKind::Neutral);

            // 消息流（最近 20 条，避免长会话拖慢帧；完整历史保留在会话态）
            let msgs: Vec<(bool, String, Option<ArtifactCard>)> = {
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
            // 底部区显式预留底带（RD-008 同类：ScrollArea auto_shrink(false) 会占满
            // 剩余高度，把输入区推出对话栏下缘；bottom_up 内 horizontal 按初始行高取
            // frame，内容仍向下溢出——改为显式矩形切带：底带按 chips 两行上限预留，
            // 消息流填中间）。
            const CHAT_INPUT_H: f32 = 178.0;
            let task_bar_h = if self.active_task().is_some()
                || self.demo_task.is_some()
                || self.demo_retry.is_some()
            {
                40.0
            } else {
                0.0
            };
            let rest = ui.available_rect_before_wrap();
            let band_top = (rest.bottom() - CHAT_INPUT_H - task_bar_h).max(rest.top());
            let scroll_rect =
                egui::Rect::from_min_max(rest.min, Pos2::new(rest.right(), band_top));
            let band_rect = egui::Rect::from_min_max(Pos2::new(rest.left(), band_top), rest.max);

            // 消息流填中间带
            let mut scroll_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(scroll_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            egui::ScrollArea::vertical()
                .id_salt("chat-scroll")
                .auto_shrink(false)
                .show(&mut scroll_ui, |ui| {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        ui.vertical(|ui| {
                            ui.set_min_width(ui.available_width());
                            ui.add_space(6.0);
                            ui.allocate_ui(sz(ui.available_width(), 18.0), |ui| {
                                ui.with_layout(
                                    Layout::centered_and_justified(egui::Direction::TopDown),
                                    |ui| {
                                        ui.label(lbl(
                                            "项目对话 · 上下文与产物持续关联",
                                            9.0,
                                            theme::FAINT,
                                        ));
                                    },
                                );
                            });
                            for (is_user, text, card) in &msgs {
                                self.render_message(ui, *is_user, text, card.clone());
                            }
                            if self.chat_pin {
                                ui.scroll_to_cursor(Some(Align::BOTTOM));
                            }
                        });
                    });
                    ui.add_space(16.0);
                });
            self.chat_pin = false;

            // 任务条 + 输入区（底带，自上而下）
            let mut band_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(band_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            let ui = &mut band_ui;
            // 任务条（协调器 / 离线演示 / 可重试）
            let coord = self
                .active_task()
                .map(|(id, label, key)| (id.to_string(), label, key));
            let demo_label = self.demo_task.as_ref().map(|t| t.label);
            let retry = self.demo_retry.filter(|_| self.demo_task.is_none() && coord.is_none());
            if coord.is_some() || demo_label.is_some() || retry.is_some() {
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
                                if let Some((task_id, label, key)) = coord {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        let (sr, _) = ui.allocate_exact_size(
                                            egui::vec2(10.0, 10.0),
                                            Sense::hover(),
                                        );
                                        let phase = ui.input(|i| i.time as f32) * 6.0;
                                        icons::paint_task_spin(
                                            ui.painter(),
                                            sr,
                                            phase,
                                            theme::ACCENT_TEXT,
                                        );
                                        ui.label(mono(
                                            format!("{}…", ellipsis(label, 18)),
                                            11.0,
                                            theme::ACCENT_TEXT,
                                        ));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.add(theme::ghost_button("取消")).clicked() {
                                            self.do_cancel(task_id, key);
                                        }
                                    });
                                } else if let Some(label) = demo_label {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        let (sr, _) = ui.allocate_exact_size(
                                            egui::vec2(10.0, 10.0),
                                            Sense::hover(),
                                        );
                                        let phase = ui.input(|i| i.time as f32) * 6.0;
                                        icons::paint_task_spin(
                                            ui.painter(),
                                            sr,
                                            phase,
                                            theme::ACCENT_TEXT,
                                        );
                                        ui.label(mono(
                                            format!("{label}…"),
                                            11.0,
                                            theme::ACCENT_TEXT,
                                        ));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.add(theme::ghost_button("取消")).clicked() {
                                            self.cancel_demo_task();
                                        }
                                    });
                                } else if let Some((label, action)) = retry {
                                    ui.label(mono("任务未完成 · 可重试", 11.0, theme::MUTED));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.add(theme::ghost_button("重试")).clicked() {
                                            self.start_demo_task(label, action);
                                        }
                                    });
                                }
                            });
                    });
                });
            }

            // 输入区
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width());
                    // 建议 chips（随研究阶段推进；原型 padding 5×8）
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().button_padding = egui::vec2(8.0, 5.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for s in self.suggestions() {
                            if ui.add(theme::chip_button(s)).clicked() {
                                self.do_ai_send_text(s.to_string());
                            }
                        }
                    });
                    ui.add_space(8.0);
                    // compose-box：外阴影先铺，再画描边盒（对齐原型 box-shadow）
                    let foreshadow = ui.available_rect_before_wrap();
                    let approx = egui::Rect::from_min_size(
                        foreshadow.min,
                        egui::vec2(foreshadow.width(), 118.0),
                    );
                    theme::paint_compose_shadow(ui.painter(), approx);
                    let compose = Frame::NONE
                        .fill(theme::GLASS_STRONG)
                        .corner_radius(CornerRadius::same(12))
                        .stroke(Stroke::new(1.0, theme::COMPOSE_STROKE))
                        .inner_margin(Margin::same(11))
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            let mut input = self.ai_input.clone();
                            let resp = ui.add(
                                area_edit(&mut input)
                                    .hint_text("描述你的研究想法，或让助手解释当前结果…")
                                    .font(FontId::proportional(theme::fs(12.0)))
                                    .desired_rows(2)
                                    .desired_width(ui.available_width())
                                    .min_size(egui::vec2(0.0, 64.0)),
                            );
                            if resp.changed() {
                                self.ai_input = input;
                            }
                            ui.add_space(7.0);
                            ui.horizontal(|ui| {
                                ui.label(lbl("↵ 发送 · Shift + ↵ 换行", 9.0, theme::FAINT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    // 原型 `.send`：圆角主色方钮 + 上箭头（非文字 ↑）
                                    let (sr, sresp) = ui.allocate_exact_size(
                                        egui::vec2(30.0, 30.0),
                                        Sense::click(),
                                    );
                                    ui.painter().rect(
                                        sr,
                                        CornerRadius::same(8),
                                        theme::ACCENT,
                                        Stroke::NONE,
                                        egui::StrokeKind::Inside,
                                    );
                                    icons::paint_send_up(
                                        ui.painter(),
                                        sr.shrink(7.0),
                                        theme::BASE,
                                    );
                                    let send = sresp.on_hover_text("发送消息");
                                    let enter = resp.has_focus()
                                        && ui.input(|i| {
                                            i.key_pressed(egui::Key::Enter) && !i.modifiers.shift
                                        });
                                    if send.clicked() || enter {
                                        let mut text = std::mem::take(&mut self.ai_input);
                                        if text.ends_with('\n') {
                                            text.pop();
                                        }
                                        self.do_ai_send_text(text);
                                    }
                                });
                            });
                        });
                    // 原型 compose-box inset 顶高光
                    let cr = compose.response.rect;
                    ui.painter().hline(
                        (cr.left() + 12.0)..=(cr.right() - 12.0),
                        cr.top() + 1.0,
                        Stroke::new(
                            1.0,
                            Color32::from_rgba_unmultiplied(255, 255, 255, 15),
                        ),
                    );
                    ui.add_space(6.0);
                    ui.allocate_ui(sz(ui.available_width(), 16.0), |ui| {
                        ui.with_layout(
                            Layout::centered_and_justified(egui::Direction::TopDown),
                            |ui| {
                                ui.label(lbl(
                                    "离线预设交互 · 计算来自合成样本 · 未接入模型",
                                    9.0,
                                    theme::FAINT,
                                ));
                            },
                        );
                    });
                    ui.add_space(6.0);
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
        card: Option<ArtifactCard>,
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
            ui.horizontal(|ui| {
                let (sp, _) = ui.allocate_exact_size(egui::Vec2::splat(10.0), Sense::hover());
                icons::paint_sparkle(ui.painter(), sp, theme::ACCENT_TEXT);
                ui.add_space(4.0);
                ui.label(lbl("研究助手", 10.0, theme::ACCENT_TEXT));
            });
            ui.label(lbl(text, 12.0, theme::TEXT2));
        }
        if let Some(card) = card {
            ui.add_space(8.0);
            let inner = ui
                .allocate_ui_with_layout(
                    sz(ui.available_width(), 0.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        // 原型 `.artifact-card`：先铺 130° 渐变再叠内容（painter 顺序）
                        Frame::NONE
                            .fill(Color32::TRANSPARENT)
                            .corner_radius(CornerRadius::same(9))
                            .stroke(Stroke::new(
                                1.0,
                                Color32::from_rgba_unmultiplied(34, 197, 94, 56),
                            ))
                            .inner_margin(Margin::symmetric(13, 10))
                            .show(ui, |ui| {
                                let w = ui.available_width();
                                let origin = ui.cursor().left_top() - egui::vec2(13.0, 10.0);
                                theme::paint_artifact_bg(
                                    ui.painter(),
                                    egui::Rect::from_min_size(
                                        origin,
                                        egui::vec2(w + 26.0, 56.0),
                                    ),
                                );
                                ui.horizontal(|ui| {
                                    let (icon_rect, _) = ui.allocate_exact_size(
                                        egui::Vec2::splat(16.0),
                                        Sense::hover(),
                                    );
                                    icons::paint_icon(
                                        ui.painter(),
                                        icon_rect,
                                        icons::for_route(card.route),
                                        theme::ACCENT_TEXT,
                                    );
                                    ui.add_space(8.0);
                                    ui.vertical(|ui| {
                                        ui.label(lbl(&card.title, 12.0, theme::TEXT));
                                        ui.label(lbl(&card.desc, 10.0, theme::MUTED));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let (ar, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(14.0),
                                            Sense::hover(),
                                        );
                                        icons::paint_icon(
                                            ui.painter(),
                                            ar,
                                            icons::IconKind::Arrow,
                                            theme::ACCENT_TEXT,
                                        );
                                    });
                                });
                            });
                    },
                )
                .response;
            let resp = inner.interact(Sense::click());
            if resp.clicked() {
                let from = self.session.route;
                self.session
                    .navigate(card.route, self.session.scroll_of(from));
            }
        }
        ui.add_space(12.0);
    }

    /// 产物卡查找（项目 ID + 消息序号 → 目标路由）。
    fn chat_card(&self, project_id: usize, msg_index: usize) -> Option<ArtifactCard> {
        self.chat_cards
            .iter()
            .rev()
            .find(|c| c.project_id == project_id && c.msg_index == msg_index)
            .cloned()
    }

    fn push_artifact(
        &mut self,
        project_id: usize,
        msg_index: usize,
        route: Route,
        title: impl Into<String>,
        desc: impl Into<String>,
    ) {
        self.chat_cards.push(ArtifactCard {
            project_id,
            msg_index,
            route,
            title: title.into(),
            desc: desc.into(),
        });
    }

    /// 建议文案（原型：`req?design?[代码,实验,报告]:[设计,解释]:[需求草稿,解释]`）。
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
        // 原型 draftNotice：相对已保存版本的脏草稿全局提示
        let dirty = self.draft_dirty_names();
        if !dirty.is_empty() {
            Self::warn_banner(
                ui,
                &format!(
                    "{}草稿尚未确认或保存。运行使用已保存版本，草稿尚未生效。",
                    dirty.join("、")
                ),
            );
        }
    }

    /// 相对当前已确认/已保存产物的脏草稿名称（需求 / 设计 / 源码）。
    fn draft_dirty_names(&self) -> Vec<&'static str> {
        let p = self.workspace.current();
        let mut names = Vec::new();
        if let Some(id) = p.active_req {
            if let Some(r) = p.reqs.get(id - 1) {
                let min_ok = self
                    .ai_req_min_amount
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|v| (v - r.min_amount).abs() < 1e-9);
                if self.ai_req_text != r.text
                    || self.ai_req_acceptance != r.acceptance
                    || (self.ai_req_alloc / 100.0 - r.allocation).abs() > 1e-9
                    || !min_ok
                {
                    names.push("需求");
                }
            }
        }
        if let Some(id) = p.active_design {
            if let Some(d) = p.designs.get(id - 1) {
                let min_ok = self
                    .ai_design_min_amount
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|v| (v - d.min_amount).abs() < 1e-9);
                if self.ai_design_note != d.note
                    || (self.ai_design_alloc / 100.0 - d.allocation).abs() > 1e-9
                    || !min_ok
                {
                    names.push("设计");
                }
            }
        }
        if let Some(id) = p.active_version {
            if let Some(v) = p.versions.get(id - 1) {
                if self.ai_code_source != v.source {
                    names.push("源码");
                }
            }
        }
        names
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

        // hero（原型 .hero：110° 线性渐变底 + 右侧 hero-orbit 双环；勿垫 GLASS 灰罩）
        let hero = Frame::NONE
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
            .stroke(Stroke::new(1.0, theme::BORDER))
            .inner_margin(Margin::same(24))
            .show(ui, |ui| {
                let hero_rect = ui.max_rect();
                // 110° 渐变底（左上亮 → 右下暗）+ 右上径向绿柔光
                {
                    let mut mesh = egui::Mesh::default();
                    mesh.colored_vertex(hero_rect.left_top(), icons::HERO_GRADIENT_FROM);
                    mesh.colored_vertex(hero_rect.right_top(), icons::HERO_GRADIENT_TO);
                    mesh.colored_vertex(hero_rect.right_bottom(), icons::HERO_GRADIENT_TO);
                    mesh.colored_vertex(hero_rect.left_bottom(), icons::HERO_GRADIENT_TO);
                    mesh.add_triangle(0, 1, 2);
                    mesh.add_triangle(0, 2, 3);
                    ui.painter().add(egui::Shape::mesh(mesh));
                    ui.painter().circle_filled(
                        Pos2::new(hero_rect.right() - 20.0 - 40.0, hero_rect.top() + 8.0),
                        88.0,
                        Color32::from_rgba_unmultiplied(34, 197, 94, 33),
                    );
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
                ui.set_min_height(170.0);
                ui.horizontal(|ui| {
                    theme::tag_ui(ui, "A股 · 日线", TagKind::Accent);
                    theme::tag_ui(
                        ui,
                        &format!("项目 {:02}", self.workspace.current().id),
                        TagKind::Neutral,
                    );
                });
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(&project_name)
                        .font(FontId::proportional(theme::fs(20.0)))
                        .color(theme::TEXT),
                );
                ui.add_space(8.0);
                ui.label(lbl(&next_reason, 12.0, theme::MUTED));
                ui.add_space(15.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button(&next_label)).clicked() {
                        let from = self.session.route;
                        self.session
                            .navigate(next_route, self.session.scroll_of(from));
                    }
                    ui.add_space(8.0);
                    // 原型 `explain`：写入概念辨析对话并跳转验证边界产物卡
                    if ui.add(theme::ghost_button("了解验证流程")).clicked() {
                        self.do_ai_send_text("了解验证流程".into());
                    }
                });
            });
        theme::paint_inset_top(ui.painter(), hero.response.rect, 16.0);
        ui.add_space(15.0);

        // 指标卡 ×3（原型 .metric-grid；@900 只保留第一张）
        let slim_metrics = {
            let win_w = ui
                .input(|i| i.raw.screen_rect.map(|r| r.width()))
                .unwrap_or(1440.0);
            layout::plan_for_width(win_w) == LayoutPlan::Slim
        };
        metric_grid(
            ui,
            &[
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
            ],
            slim_metrics,
        );
        ui.add_space(15.0);

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
        panel_flush(
            ui,
            "研究路径",
            Some(("同一项目 · 全程关联", TagKind::Neutral)),
            |ui| {
                let n = stages.len();
                for (i, (route, title, desc, state, kind)) in stages.iter().enumerate() {
                    let last = i + 1 == n;
                    let w = ui.available_width();
                    let row = ui.allocate_ui_with_layout(
                        egui::vec2(w, 62.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(ui.available_width());
                            let r = ui.max_rect();
                            if ui.rect_contains_pointer(r) {
                                ui.painter().rect_filled(r, CornerRadius::ZERO, theme::WHITE_03);
                            }
                            if !last {
                                ui.painter().line_segment(
                                    [r.left_bottom(), r.right_bottom()],
                                    Stroke::new(1.0, theme::BORDER),
                                );
                            }
                            ui.add_space(18.0);
                            // 原型 `.step-num`：空心绿圈 + accent-text 等宽步号
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(25.0, 25.0), Sense::hover());
                            let c = rect.center();
                            ui.painter().circle_stroke(
                                c,
                                12.5,
                                Stroke::new(1.0, theme::ACCENT_STEP_STROKE),
                            );
                            ui.painter().text(
                                c,
                                ALIGN2_CENTER,
                                format!("{:02}", i + 1),
                                FontId::monospace(theme::fs(10.0)),
                                theme::ACCENT_TEXT,
                            );
                            ui.add_space(13.0);
                            ui.vertical(|ui| {
                                ui.label(lbl(title, 12.0, theme::TEXT));
                                ui.add_space(5.0);
                                ui.label(lbl(desc, 10.0, theme::MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_space(18.0);
                                let (ir, _) =
                                    ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
                                icons::paint_icon(
                                    ui.painter(),
                                    ir,
                                    icons::IconKind::Arrow,
                                    theme::FAINT,
                                );
                                ui.add_space(8.0);
                                theme::tag_ui(ui, state, *kind);
                            });
                        },
                    );
                    if row.response.interact(Sense::click()).clicked() {
                        let from = self.session.route;
                        self.session.navigate(*route, self.session.scroll_of(from));
                    }
                }
            },
        );

        ui.add_space(10.0);
        ui.label(lbl(
            "当前为离线原型。你可以通过右侧对话推进，也可以直接打开文档、代码与实验。所有演示结果均标注来源。",
            10.0,
            theme::FAINT,
        ));
    }

    // ---- 需求文档 ------------------------------------------------------------
    fn render_requirements(&mut self, ui: &mut egui::Ui) {
        let req_saved = {
            let p = self.workspace.current();
            p.active_req.map(|id| p.reqs[id - 1].clone())
        };
        let req_badge = {
            let p = self.workspace.current();
            p.active_req.map(|id| format!("R{id} 已确认"))
        };
        let req_badge_kind = if req_badge.is_some() {
            TagKind::Accent
        } else {
            TagKind::Neutral
        };
        let req_badge_text = req_badge.unwrap_or_else(|| "未确认草稿".into());
        page_header_ex(
            ui,
            "阶段 01 / 定义问题",
            "研究需求",
            "把想法转成明确的约束与可检验的标准。",
            Some((req_badge_text.as_str(), req_badge_kind)),
            None,
        );

        panel(
            ui,
            "需求文档",
            Some(("可直接编辑", TagKind::Neutral)),
            |ui| {
                ui.label(field("研究目标与处理逻辑"));
                ui.add_space(7.0);
                let mut text = self.ai_req_text.clone();
                if ui
                    .add(area_edit(&mut text).desired_rows(4).desired_width(ui.available_width()))
                    .changed()
                {
                    self.ai_req_text = text;
                }
                ui.add_space(16.0);
                ui.label(field("验收标准"));
                ui.add_space(7.0);
                let mut acc = self.ai_req_acceptance.clone();
                if ui
                    .add(area_edit(&mut acc).desired_rows(3).desired_width(ui.available_width()))
                    .changed()
                {
                    self.ai_req_acceptance = acc;
                }
                ui.add_space(16.0);
                two_field_row(
                    ui,
                    |ui| {
                        field_label(ui, "目标投入比例（%）");
                        let mut alloc_s = format!("{:.0}", self.ai_req_alloc);
                        if ui
                            .add(line_edit(&mut alloc_s).desired_width(ui.available_width()))
                            .changed()
                        {
                            if let Ok(v) = alloc_s.parse::<f64>() {
                                self.ai_req_alloc = v;
                            }
                        }
                    },
                    |ui| {
                        field_label(ui, "最低成交额（万元）");
                        let mut v = self.ai_req_min_amount.clone();
                        if ui
                            .add(line_edit(&mut v).desired_width(ui.available_width()))
                            .changed()
                        {
                            self.ai_req_min_amount = v;
                        }
                    },
                );
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let confirm = if req_saved.is_some() {
                        "确认并保存新需求版本"
                    } else {
                        "确认需求"
                    };
                    if ui.add(theme::primary_button(confirm)).clicked() {
                        self.do_confirm_requirement();
                    }
                    ui.add_space(8.0);
                    if ui.add(theme::ghost_button("下载文档")).clicked() {
                        self.export_requirement_draft();
                    }
                });
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
                        format!("需求 R{} · 投入 {:.0}%", r.id, r.allocation * 100.0),
                        p.active_req == Some(r.id),
                    )
                })
                .collect()
        };
        panel(ui, "确认历史", None, |ui| {
            if history.is_empty() {
                ui.label(note("还没有确认版本。草稿保留在当前项目内，刷新会重置。"));
            } else {
                for (id, desc, active) in history {
                    let label = if active {
                        format!("{desc}  ·  当前")
                    } else {
                        desc
                    };
                    if ui
                        .add(
                            theme::default_button(format!("{label}    查看只读版本 →"))
                                .min_size(egui::vec2(ui.available_width(), 40.0)),
                        )
                        .clicked()
                    {
                        self.view_req = Some(id);
                    }
                    ui.add_space(6.0);
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
        if !req_ok {
            page_header(
                ui,
                "阶段 02 / 处理逻辑",
                "策略设计",
                "先明确需求，再设计处理逻辑。",
            );
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
        page_header_ex(
            ui,
            "阶段 02 / 处理逻辑",
            "策略设计",
            "文档、处理图与代码，共用同一份策略逻辑。",
            Some((design_state.as_str(), kind)),
            None,
        );

        panel(ui, "设计说明", Some((&design_state, kind)), |ui| {
            let mut note_text = self.ai_design_note.clone();
                if ui
                    .add(
                        area_edit(&mut note_text)
                            .desired_rows(5)
                            .desired_width(ui.available_width()),
                    )
                    .changed()
                {
                    self.ai_design_note = note_text;
                }
                ui.add_space(16.0);
                two_field_row(
                    ui,
                    |ui| {
                        field_label(ui, "投入比例（%）");
                        let mut alloc_s = format!("{:.0}", self.ai_design_alloc);
                        if ui
                            .add(line_edit(&mut alloc_s).desired_width(ui.available_width()))
                            .changed()
                        {
                            if let Ok(v) = alloc_s.parse::<f64>() {
                                self.ai_design_alloc = v;
                            }
                        }
                    },
                    |ui| {
                        field_label(ui, "最低成交额（万元）");
                        let mut v = self.ai_design_min_amount.clone();
                        if ui
                            .add(line_edit(&mut v).desired_width(ui.available_width()))
                            .changed()
                        {
                            self.ai_design_min_amount = v;
                        }
                    },
                );
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(theme::primary_button("保存设计版本")).clicked() {
                    self.do_generate_design();
                }
                ui.add_space(8.0);
                if ui.add(theme::default_button("从需求生成设计")).clicked() {
                    self.start_demo_task("生成策略设计", DemoAction::DesignDraft);
                }
                ui.add_space(8.0);
                if ui.add(theme::ghost_button("下载设计")).clicked() {
                    self.export_design_draft();
                }
            });
        });

        ui.add_space(6.0);
        {
            let mut go_flow = false;
            let mut go_seq = false;
            panel_with_right(
                ui,
                "策略处理图",
                |ui| {
                    // 原型 panel-head：流程图 → 时序图
                    if ui.add(tab_button("流程图", self.design_tab_flow)).clicked() {
                        go_flow = true;
                    }
                    ui.add_space(4.0);
                    if ui
                        .add(tab_button("时序图", !self.design_tab_flow))
                        .clicked()
                    {
                        go_seq = true;
                    }
                },
                |ui| {
                    if self.design_tab_flow {
                        if let Some(i) = paint_flow_diagram(ui, self.design_node) {
                            self.design_node = i;
                        }
                    } else if let Some(i) = paint_sequence_diagram(ui, self.design_node) {
                        self.design_node = i;
                    }
                    ui.add_space(8.0);
                    ui.label(note(
                        "点击节点查看接口含义与源码映射。图描述预期行为；运行事件描述实际结果。",
                    ));
                },
            );
            if go_flow {
                self.design_tab_flow = true;
            }
            if go_seq {
                self.design_tab_flow = false;
            }
        }

        let (name, input, output, line, nid) = {
            let n = &NODES[self.design_node.min(NODES.len() - 1)];
            let (input, output) = n.3.split_once(" → ").unwrap_or((n.3, ""));
            (n.1, input.to_string(), output.to_string(), n.4, n.0)
        };
        panel(
            ui,
            &format!("节点检查 · {name}"),
            Some((&format!("节点 {nid}"), TagKind::Neutral)),
            |ui| {
                kv(
                    ui,
                    &[
                        ("输入", input),
                        ("输出", output),
                        (
                            "设计参数",
                            format!(
                                "投入 {:.0}% · 成交额 ≥ {} 万元",
                                self.ai_design_alloc, self.ai_design_min_amount
                            ),
                        ),
                    ],
                );
                ui.add_space(10.0);
                if ui
                    .add(theme::small_default_button(&format!("定位示例源码第 {line} 行")))
                    .clicked()
                {
                    self.jump_code_line = Some(line);
                    let from = self.session.route;
                    self.session
                        .navigate(Route::Develop, self.session.scroll_of(from));
                }
            },
        );
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
        let header_kind = if version_state.starts_with('v') {
            TagKind::Accent
        } else {
            TagKind::Neutral
        };
        page_header_ex(
            ui,
            "阶段 03 / 策略实现",
            "策略开发",
            "保存明确的版本，再把它交给实验与验证。",
            Some((version_state.as_str(), header_kind)),
            None,
        );
        if !design_ok {
            Self::warn_banner(ui, "当前没有匹配需求的设计。请先保存设计版本。");
        }

        let checked_ok = self
            .ai_code_checked
            .as_ref()
            .is_some_and(|c| c == &self.ai_code_source);
        let line_n = self.ai_code_source.lines().count();
        // 原型 develop：panel-head（文件名）→ .code 深色编辑区 → panel-head（行数）→ pad 按钮
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
                            let (ir, _) =
                                ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
                            icons::paint_icon(
                                ui.painter(),
                                ir,
                                icons::IconKind::Code,
                                theme::MUTED,
                            );
                            ui.add_space(8.0);
                            ui.label(lbl("策略示例.py", 10.0, theme::MUTED));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(theme::small_ghost_button("下载")).clicked() {
                                    self.export_strategy_source();
                                }
                                ui.add_space(6.0);
                                theme::tag_ui(
                                    ui,
                                    if checked_ok {
                                        "映射检查通过"
                                    } else {
                                        "草稿待检查"
                                    },
                                    if checked_ok {
                                        TagKind::Accent
                                    } else {
                                        TagKind::Neutral
                                    },
                                );
                            });
                        });
                    });
                {
                    let hr = head.response.rect;
                    ui.painter().line_segment(
                        [
                            Pos2::new(hr.left(), hr.bottom()),
                            Pos2::new(hr.right(), hr.bottom()),
                        ],
                        Stroke::new(1.0, theme::BORDER),
                    );
                }
                Frame::NONE
                    .fill(theme::CODE_BG)
                    .inner_margin(Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if let Some(line) = self.jump_code_line {
                            ui.label(lbl(
                                format!("▶ 定位到第 {line} 行（设计节点 / 事件检查器）"),
                                11.0,
                                theme::ACCENT_TEXT,
                            ));
                            ui.add_space(8.0);
                            if ui.add(theme::small_ghost_button("清除定位")).clicked() {
                                self.jump_code_line = None;
                            }
                            ui.add_space(8.0);
                        }
                        let mut src = self.ai_code_source.clone();
                        let te = egui::TextEdit::multiline(&mut src)
                            .desired_rows(16)
                            .desired_width(ui.available_width())
                            .font(FontId::monospace(theme::fs(11.0)))
                            .text_color(theme::CODE_FG)
                            .frame(Frame::NONE)
                            .code_editor();
                        if ui.add(te).changed() {
                            self.ai_code_source = src;
                        }
                    });
                let foot = Frame::NONE
                    .inner_margin(Margin {
                        left: 19,
                        right: 19,
                        top: 12,
                        bottom: 12,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(note("Python 示例文本 · 不执行任意代码"));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(note(format!("{line_n} 行")));
                            });
                        });
                    });
                {
                    let hr = foot.response.rect;
                    ui.painter().line_segment(
                        [
                            Pos2::new(hr.left(), hr.bottom()),
                            Pos2::new(hr.right(), hr.bottom()),
                        ],
                        Stroke::new(1.0, theme::BORDER),
                    );
                }
                Frame::NONE.inner_margin(Margin::same(19)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.add(theme::default_button("检查草稿")).clicked() {
                            self.do_check_code();
                        }
                        ui.add_space(8.0);
                        let save = ui.add_enabled(
                            checked_ok && design_ok,
                            theme::primary_button("保存版本"),
                        );
                        if save.clicked() {
                            self.do_save_version();
                        }
                        ui.add_space(8.0);
                        if ui.add(theme::ghost_button("生成初始代码")).clicked() {
                            self.start_demo_task("生成策略代码", DemoAction::GenerateCode);
                        }
                    });
                });
            });
        ui.add_space(12.0);

        panel(ui, "资金约束修复", None, |ui| {
            ui.label(note(
                "原始示例按信号收盘价计算数量，执行日价格和费用可能导致资金不足。修复将资金检查放到下一交易日的成交阶段。",
            ));
            ui.add_space(10.0);
            if self.show_code_diff {
                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(34, 197, 94, 15))
                    .stroke(Stroke::new(0.0, theme::BORDER))
                    .inner_margin(Margin {
                        left: 12,
                        right: 12,
                        top: 12,
                        bottom: 12,
                    })
                    .show(ui, |ui| {
                        let r = ui.max_rect();
                        ui.painter().vline(
                            r.left(),
                            r.y_range(),
                            Stroke::new(2.0, theme::ACCENT),
                        );
                        ui.label(mono(
                            "− 按信号收盘价计算数量，未预留费用\n＋ 执行阶段使用开盘价，先预留 5 元费用，再取整手数量",
                            11.0,
                            theme::TEXT2,
                        ));
                    });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button("应用修复到草稿")).clicked() {
                        self.ai_code_source = crate::workspace::DRAFT_CODE_FIXED.into();
                        self.ai_code_checked = None;
                        self.show_code_diff = false;
                        self.record("已应用资金约束修复草稿 · 需重新检查");
                    }
                    ui.add_space(8.0);
                    if ui.add(theme::ghost_button("取消差异")).clicked() {
                        self.show_code_diff = false;
                    }
                });
            } else if ui.add(theme::default_button("查看修复差异")).clicked() {
                self.show_code_diff = true;
                self.tell(
                    "建议在成交阶段使用可见的开盘价，并预留费用。工作区展示修改差异；应用后需要重新检查、保存版本和运行实验。",
                    Some(Route::Develop),
                );
            }
        });

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
                ui.label(note("检查草稿并保存后，版本会出现在这里。"));
            }
            for (id, req, design, fresh, active) in &versions {
                let variant = self
                    .workspace
                    .current()
                    .versions
                    .get(id - 1)
                    .map(|v| {
                        if crate::workspace::code_preset_variant(&v.source) == Some(2) {
                            "资金约束修复"
                        } else {
                            "原始"
                        }
                    })
                    .unwrap_or("原始");
                let row = ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(mono(
                            format!("v{id} · {variant}  R{req} / D{design}"),
                            11.0,
                            theme::TEXT2,
                        )).fill(Color32::TRANSPARENT).stroke(Stroke::NONE))
                        .clicked()
                    {
                        self.workspace.current_mut().active_version = Some(*id);
                    }
                    ui.add_space(8.0);
                    theme::tag_ui(
                        ui,
                        if *active {
                            "已选中"
                        } else if *fresh {
                            "可选"
                        } else {
                            "过期"
                        },
                        if *active {
                            TagKind::Accent
                        } else {
                            TagKind::Neutral
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(theme::small_default_button("查看")).clicked() {
                            self.view_source = self
                                .workspace
                                .current()
                                .versions
                                .get(id - 1)
                                .map(|v| (v.id, v.source.clone(), None));
                        }
                    });
                });
                let _ = row;
                ui.add_space(6.0);
            }
            if !versions.is_empty() {
                ui.add_space(8.0);
                let can_run = {
                    let p = self.workspace.current();
                    p.active_version
                        .map(|id| self.workspace.version_fresh(id))
                        .unwrap_or(false)
                        && !self.ai_run.watch.is_active()
                };
                ui.add_enabled_ui(can_run, |ui| {
                    if ui
                        .add(theme::primary_button("运行选中版本"))
                        .clicked()
                    {
                        self.do_ai_run();
                    }
                });
            }
        });
    }

    // ---- 事件调试 ------------------------------------------------------------
    fn render_debug(&mut self, ui: &mut egui::Ui) {
        let exps: Vec<(usize, usize, u32, i64)> = self
            .workspace
            .current()
            .experiments
            .iter()
            .map(|e| {
                let (rej, held) = e
                    .demo
                    .as_ref()
                    .map(|d| (d.rejected, d.held))
                    .unwrap_or((0, 0));
                (e.id, e.version_id, rej, held)
            })
            .collect();
        let debug_badge = exps
            .last()
            .map(|(id, ver, _, _)| format!("E{id} / v{ver}"));
        page_header_ex(
            ui,
            "阶段 04 / 解释运行行为",
            "事件调试",
            if exps.is_empty() {
                "沿着一笔信号与订单，找到问题发生的位置。"
            } else {
                "回放冻结实验，查看每一步的输入、输出与账户变化。"
            },
            debug_badge.as_deref().map(|s| (s, TagKind::Accent)),
            None,
        );
        if exps.is_empty() {
            if empty_panel(
                ui,
                "还没有运行事件",
                "先保存一个策略版本并运行合成实验。",
                Some("打开策略开发"),
            ) {
                let from = self.session.route;
                self.session
                    .navigate(Route::Develop, self.session.scroll_of(from));
            }
            return;
        }

        if self.debug_run_id == 0 || !exps.iter().any(|(id, _, _, _)| *id == self.debug_run_id)
        {
            self.debug_run_id = exps.last().map(|(id, _, _, _)| *id).unwrap_or(0);
            self.debug_event_index = 0;
            self.debug_playing = false;
        }
        let selected = exps
            .iter()
            .find(|(id, _, _, _)| *id == self.debug_run_id)
            .cloned()
            .or_else(|| exps.last().cloned())
            .unwrap_or((0, 0, 0, 0));
        let demo = self
            .workspace
            .current()
            .experiments
            .iter()
            .find(|e| e.id == selected.0)
            .and_then(|e| e.demo.clone());
        let filtered: Vec<crate::demo_sim::DemoEvent> = demo
            .as_ref()
            .map(|d| {
                d.events
                    .iter()
                    .filter(|ev| self.debug_filter.is_empty() || ev.node == self.debug_filter)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if !filtered.is_empty() && self.debug_event_index >= filtered.len() {
            self.debug_event_index = filtered.len() - 1;
        }
        let has_step_events = !filtered.is_empty();
        let mut run_changed = false;
        let mut rewind = false;
        let mut step = false;
        let mut toggle_play = false;
        let mut pick_seq: Option<usize> = None;
        let mut jump_line: Option<u32> = None;

        panel(ui, "事件回放", None, |ui| {
            ui.horizontal(|ui| {
                let status = if selected.2 > 0 {
                    "订单拒绝"
                } else {
                    "已完成"
                };
                let label = format!("E{} · v{} · {status}", selected.0, selected.1);
                let prev = self.debug_run_id;
                egui::ComboBox::from_id_salt("debug-run")
                    .selected_text(label)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for (id, ver, rej, _) in &exps {
                            let st = if *rej > 0 { "订单拒绝" } else { "已完成" };
                            ui.selectable_value(
                                &mut self.debug_run_id,
                                *id,
                                format!("E{id} · v{ver} · {st}"),
                            );
                        }
                    });
                if self.debug_run_id != prev {
                    run_changed = true;
                }
                ui.add_space(8.0);
                ui.add_enabled_ui(has_step_events, |ui| {
                    if ui
                        .add(theme::small_default_button("回到起点"))
                        .on_disabled_hover_text("当前实验无逐步事件")
                        .clicked()
                    {
                        rewind = true;
                    }
                    ui.add_space(4.0);
                    if ui
                        .add(theme::small_default_button("单步 →"))
                        .on_disabled_hover_text("当前实验无逐步事件")
                        .clicked()
                    {
                        step = true;
                    }
                    ui.add_space(4.0);
                    let play_label = if self.debug_playing { "暂停" } else { "播放" };
                    if ui
                        .add(theme::small_primary_button(play_label))
                        .on_disabled_hover_text("当前实验无逐步事件")
                        .clicked()
                    {
                        toggle_play = true;
                    }
                });
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                let all = self.debug_filter.is_empty();
                if ui.add(tab_button("全部事件", all)).clicked() {
                    self.debug_filter.clear();
                    self.debug_event_index = 0;
                }
                for n in &NODES {
                    let on = self.debug_filter == n.0;
                    if ui.add(tab_button(n.1, on)).clicked() {
                        self.debug_filter = n.0.to_string();
                        self.debug_event_index = 0;
                    }
                }
            });
            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .max_height(255.0)
                .show(ui, |ui| {
                    egui::Grid::new("debug-events")
                        .num_columns(4)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            for h in ["序号", "时点", "节点 / 标的", "结果"] {
                                ui.label(mono(h, 10.0, theme::FAINT));
                            }
                            ui.end_row();
                            if filtered.is_empty() {
                                ui.label(note("当前筛选下没有事件。"));
                                ui.end_row();
                            } else {
                                for (i, ev) in filtered.iter().enumerate() {
                                    let selected_row = i == self.debug_event_index;
                                    let node_name = NODES
                                        .iter()
                                        .find(|n| n.0 == ev.node)
                                        .map(|n| n.1)
                                        .unwrap_or(ev.node.as_str());
                                    let row = format!(
                                        "{:02}  {}  {} / {}  {}",
                                        ev.seq,
                                        &ev.visible_at[5.min(ev.visible_at.len())..],
                                        node_name,
                                        ev.symbol,
                                        ev.status
                                    );
                                    let resp = ui.selectable_label(selected_row, mono(
                                        format!("{:02}", ev.seq),
                                        11.0,
                                        theme::TEXT2,
                                    ));
                                    if resp.clicked() {
                                        pick_seq = Some(i);
                                    }
                                    ui.label(mono(
                                        ev.visible_at
                                            .get(5..)
                                            .unwrap_or(ev.visible_at.as_str()),
                                        11.0,
                                        theme::MUTED,
                                    ));
                                    ui.vertical(|ui| {
                                        ui.label(lbl(node_name, 11.0, theme::TEXT2));
                                        ui.label(mono(&ev.symbol, 10.0, theme::MUTED));
                                    });
                                    let bad = ev.status.contains("拒绝");
                                    ui.label(lbl(
                                        &ev.status,
                                        11.0,
                                        if bad { theme::RED } else { theme::TEXT2 },
                                    ));
                                    let _ = row;
                                    ui.end_row();
                                }
                            }
                        });
                });
            ui.add_space(8.0);
            ui.label(note(
                "播放已记录的事件；此处不是实时策略断点调试器。",
            ));
        });

        if run_changed {
            self.debug_event_index = 0;
            self.debug_playing = false;
        }
        if rewind {
            self.debug_event_index = 0;
            self.debug_playing = false;
        }
        if step {
            let n = filtered.len();
            if n > 0 {
                self.debug_event_index = (self.debug_event_index + 1).min(n - 1);
            }
            self.debug_playing = false;
        }
        if toggle_play {
            self.debug_playing = !self.debug_playing;
            self.debug_play_at = None;
        }
        if let Some(i) = pick_seq {
            self.debug_event_index = i;
            self.debug_playing = false;
        }

        ui.add_space(6.0);
        let eid = selected.0;
        let evidence = format!("证据来自 E{eid}");
        let current = filtered.get(self.debug_event_index).cloned();
        panel(
            ui,
            "事件检查器",
            Some((evidence.as_str(), TagKind::Neutral)),
            |ui| {
                if let Some(ev) = &current {
                    ui.horizontal(|ui| {
                        theme::tag_ui(ui, &ev.symbol, TagKind::Neutral);
                        let bad = ev.status.contains("拒绝");
                        theme::tag_ui(
                            ui,
                            &ev.status,
                            if bad { TagKind::Bad } else { TagKind::Accent },
                        );
                        ui.label(note(&ev.visible_at));
                    });
                    ui.add_space(10.0);
                    ui.columns(2, |cols| {
                        cols[0].label(note("输入"));
                        cols[0].label(mono(&ev.input_json, 11.0, theme::TEXT2));
                        cols[1].label(note("输出"));
                        cols[1].label(mono(&ev.output_json, 11.0, theme::TEXT2));
                    });
                    ui.add_space(10.0);
                    if ui
                        .add(theme::small_default_button(format!(
                            "定位冻结源码 · 第 {} 行",
                            ev.line
                        )))
                        .clicked()
                    {
                        jump_line = Some(ev.line);
                    }
                } else {
                    ui.label(note("当前筛选下没有事件。"));
                }
            },
        );
        if let Some(line) = jump_line {
            let src = self
                .workspace
                .current()
                .experiments
                .iter()
                .find(|e| e.id == selected.0)
                .and_then(|e| {
                    self.workspace
                        .current()
                        .versions
                        .get(e.version_id - 1)
                        .map(|v| (v.id, v.source.clone()))
                });
            if let Some((id, source)) = src {
                self.view_source = Some((id, source, Some(line)));
            }
        }

        ui.add_space(6.0);
        let conclusion = if selected.2 > 0 {
            "仓位数量按旧参考价估算，执行价格与费用使订单成本超过现金。查看成交事件，再应用开发区的修复。"
        } else if selected.3 > 0 {
            "资金约束已满足。接下来检查回测指标与验证证据，不能只根据一次收益判断策略有效。"
        } else {
            "没有形成持仓。请检查股票池、信号条件与投入比例。"
        };
        panel(ui, "这次运行告诉我们什么", None, |ui| {
            ui.label(note(conclusion));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(theme::default_button("查看修复差异")).clicked() {
                    self.show_code_diff = true;
                    let from = self.session.route;
                    self.session
                        .navigate(Route::Develop, self.session.scroll_of(from));
                }
                ui.add_space(8.0);
                if ui.add(theme::ghost_button("回测实验")).clicked() {
                    let from = self.session.route;
                    self.session
                        .navigate(Route::Experiments, self.session.scroll_of(from));
                }
            });
        });

        // 原型 debug 无「实验比较归因」主面板；比较入口收起到折叠区，主路径留给事件回放
        ui.add_space(6.0);
        let ids: Vec<usize> = exps.iter().map(|(id, _, _, _)| *id).collect();
        let cmp_a = self.debug_cmp[0];
        let cmp_b = self.debug_cmp[1];
        ui.collapsing("实验比较归因（高级）", |ui| {
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
            ui.label(note(
                "同输入实验的分歧归因代码；跨输入实验先列出输入差异，不归因代码。",
            ));
        });
        self.poll(PageKey::Run);
    }

    // ---- 回测实验 ------------------------------------------------------------
    fn render_experiments(&mut self, ui: &mut egui::Ui) {
        // id, ver, ret, cash, held, rejected, final_nav, return_pct, drawdown
        let exps: Vec<(
            usize,
            usize,
            Option<String>,
            Option<f64>,
            Option<i64>,
            Option<u32>,
            Option<f64>,
            Option<f64>,
            Option<f64>,
        )> = {
            let p = self.workspace.current();
            p.experiments
                .iter()
                .map(|e| {
                    let d = e.demo.as_ref();
                    (
                        e.id,
                        e.version_id,
                        e.total_return.clone(),
                        d.map(|x| x.cash),
                        d.map(|x| x.held),
                        d.map(|x| x.rejected),
                        d.map(|x| x.final_nav),
                        d.map(|x| x.return_pct),
                        d.map(|x| x.drawdown_pct),
                    )
                })
                .collect()
        };
        let empty_runs = exps.is_empty();
        let last = exps.last().cloned();
        let last_id = last.as_ref().map(|e| e.0).unwrap_or(0);
        let ret_pct = last
            .as_ref()
            .and_then(|e| e.7.map(|p| format!("{p:.2}%")).or_else(|| e.2.as_deref().map(format_return_pct)))
            .unwrap_or_else(|| "—".into());
        let nav_text = last
            .as_ref()
            .and_then(|e| {
                e.6.map(|v| format!("{v:.2}"))
                    .or_else(|| e.2.as_deref().and_then(nav_from_return).map(|v| format!("{v:.2}")))
            })
            .unwrap_or_else(|| "—".into());
        let dd_text = last
            .as_ref()
            .and_then(|e| e.8.map(|d| format!("{d:.2}%")))
            .unwrap_or_else(|| "—".into());
        if page_header_ex(
            ui,
            "实验 / 历史模拟",
            "回测实验",
            if empty_runs {
                "冻结输入、运行模拟，再比较结果。"
            } else {
                "回测回答历史模拟表现，验证决定证据是否充分。"
            },
            None,
            if empty_runs {
                None
            } else {
                Some("运行当前版本")
            },
        ) {
            self.do_ai_run();
        }
        if empty_runs {
            if empty_panel(
                ui,
                "准备你的第一次实验",
                "选择一个已保存且未过期的策略版本。合成计算将真实产出账户与事件记录。",
                Some("运行合成实验"),
            ) {
                self.do_ai_run();
            }
            self.poll(PageKey::Run);
            return;
        }

        let nav_label = format!("E{last_id} 期末净值");
        metric_grid(
            ui,
            &[
                (
                    nav_label.as_str(),
                    nav_text,
                    "初始 10,000.00 元".to_string(),
                ),
                (
                    "累计收益",
                    ret_pct,
                    "仅合成样本 · 不年化".to_string(),
                ),
                ("最大回撤", dd_text, "三个估值点".to_string()),
            ],
            false,
        );
        ui.add_space(4.0);

        self.render_equity_chart(ui);

        ui.add_space(6.0);
        let mut export_clicked = false;
        let mut go_debug: Option<usize> = None;
        let mut snap_msg: Option<String> = None;
        let mut go_div = false;
        panel_with_right(
            ui,
            "实验记录",
            |ui| {
                if ui.add(theme::small_default_button("导出 JSON")).clicked() {
                    export_clicked = true;
                }
            },
            |ui| {
                egui::Grid::new("exp-grid")
                    .num_columns(5)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for head in ["实验 / 版本", "收益", "期末现金", "持仓 / 拒绝", "证据"] {
                            ui.label(mono(head, 10.0, theme::FAINT));
                        }
                        ui.end_row();
                        for (id, ver, ret, cash, held, rej, _fin, rpct, _) in &exps {
                            ui.label(mono(format!("E{id} / v{ver}"), 11.0, theme::TEXT2));
                            let pct = rpct
                                .map(|p| format!("{p:.2}%"))
                                .or_else(|| ret.as_deref().map(format_return_pct))
                                .unwrap_or_else(|| "—".into());
                            ui.label(mono(pct, 11.0, theme::TEXT2));
                            ui.label(mono(
                                cash.map(|c| format!("{c:.2}")).unwrap_or_else(|| "—".into()),
                                11.0,
                                theme::TEXT2,
                            ));
                            ui.label(mono(
                                match (held, rej) {
                                    (Some(h), Some(r)) => format!("{h} 股 / {r}"),
                                    _ => "—".into(),
                                },
                                11.0,
                                theme::TEXT2,
                            ));
                            ui.horizontal(|ui| {
                                if ui.add(theme::small_default_button("调试")).clicked() {
                                    go_debug = Some(*id);
                                }
                                if ui.add(theme::small_ghost_button("快照")).clicked() {
                                    snap_msg = Some(format!(
                                        "实验 E{id} 冻结快照：v{ver} · 合成演示证据包。"
                                    ));
                                }
                            });
                            ui.end_row();
                        }
                    });
            },
        );
        if export_clicked {
            self.export_experiments_json();
        }
        if let Some(id) = go_debug {
            self.debug_run_id = id;
            self.debug_event_index = 0;
            let from = self.session.route;
            self.session
                .navigate(Route::Debug, self.session.scroll_of(from));
        }
        if let Some(msg) = snap_msg {
            self.tell(msg, None);
        }

        ui.add_space(6.0);
        let divergence = self.workspace.first_event_divergence();
        panel(ui, "首个事件分歧", None, |ui| {
            match &divergence {
                None if exps.len() < 2 => {
                    ui.label(note(
                        "运行两个版本后，自动比较事件输出，定位第一个不同之处。",
                    ));
                }
                None => {
                    ui.label(note("最近两次实验事件输出一致，或缺少合成事件证据。"));
                }
                Some((a, b, e, f)) => {
                    let node_name = NODES
                        .iter()
                        .find(|n| n.0 == e.node)
                        .map(|n| n.1)
                        .unwrap_or(e.node.as_str());
                    ui.label(note(format!(
                        "{node_name} · {} · {}",
                        e.symbol, e.date
                    )));
                    ui.add_space(6.0);
                    ui.label(mono(
                        format!(
                            "E{a}：{}\nE{b}：{}",
                            e.output_json,
                            f.as_ref()
                                .map(|x| x.output_json.as_str())
                                .unwrap_or("事件缺失")
                        ),
                        11.0,
                        theme::TEXT2,
                    ));
                    ui.add_space(8.0);
                    if ui
                        .add(theme::small_default_button("定位分歧事件"))
                        .clicked()
                    {
                        go_div = true;
                    }
                }
            }
        });
        if go_div {
            if let Some((_, b, e, _)) = divergence {
                self.debug_run_id = b;
                self.debug_filter.clear();
                self.debug_playing = false;
                let ix = self
                    .workspace
                    .current()
                    .experiments
                    .iter()
                    .find(|x| x.id == b)
                    .and_then(|x| x.demo.as_ref())
                    .and_then(|d| {
                        d.events
                            .iter()
                            .position(|ev| ev.node == e.node && ev.symbol == e.symbol && ev.date == e.date)
                    })
                    .unwrap_or(0);
                self.debug_event_index = ix;
                let from = self.session.route;
                self.session
                    .navigate(Route::Debug, self.session.scroll_of(from));
            }
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add(theme::primary_button("生成当前版本验证报告"))
                .clicked()
            {
                self.do_make_report();
            }
            ui.add_space(8.0);
            theme::tag_ui(ui, "合成数据 · 非真实回测", TagKind::Warn);
        });

        ui.add_space(6.0);
        // 协调器「运行比较」默认折叠，避免挤占原型主路径（净值 / 实验记录 / 验证）
        ui.collapsing("运行比较（协调器）", |ui| {
            ui.label(note(
                "2..5 个运行 ID（逗号分割；ID 见任务记录），跨输入比较不归因代码。",
            ));
            let mut text = self.compare_form.run_ids_text.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .desired_width(ui.available_width())
                        .font(FontId::monospace(theme::fs(12.0))),
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
        ui.collapsing("运行参数（协调器）", |ui| {
            ui.horizontal(|ui| {
                ui.label(mono("fast", 11.0, theme::MUTED));
                ui.add(egui::DragValue::new(&mut self.run_form.fast));
                ui.label(mono("slow", 11.0, theme::MUTED));
                ui.add(egui::DragValue::new(&mut self.run_form.slow));
                ui.label(field("资金"));
                let mut c = self.run_form.capital.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut c).desired_width(100.0))
                    .changed()
                {
                    self.run_form.capital = c;
                }
            });
            render_watch(ui, &self.ai_run);
        });

        self.poll(PageKey::Run);
    }

    /// 净值比较面板：优先离线合成 `demo.equity`（对齐原型）；否则协调器任务曲线。
    fn render_equity_chart(&mut self, ui: &mut egui::Ui) {
        let demo_latest: Vec<(usize, usize, crate::demo_sim::DemoRun)> = self
            .workspace
            .current()
            .experiments
            .iter()
            .rev()
            .filter_map(|e| {
                e.demo
                    .as_ref()
                    .map(|d| (e.id, e.version_id, d.clone()))
            })
            .take(2)
            .collect();
        let task_latest = self.workspace.latest_task_experiments(2); // 新→旧
        let key = if !demo_latest.is_empty() {
            (
                demo_latest.get(1).map(|x| format!("demo:{}", x.0)),
                demo_latest.first().map(|x| format!("demo:{}", x.0)),
            )
        } else {
            (
                task_latest.get(1).map(|x| x.2.clone()),
                task_latest.first().map(|x| x.2.clone()),
            )
        };
        if self.equity_chart.key != key {
            let mut series: Vec<(String, Vec<crate::bridge::EquityPoint>)> = Vec::new();
            let mut error = None;
            let mut pending = false;
            if !demo_latest.is_empty() {
                let dates = ["2026-01-05", "2026-01-06", "2026-01-07"];
                for (id, ver, demo) in demo_latest.iter().rev() {
                    let pts: Vec<_> = demo
                        .equity
                        .iter()
                        .enumerate()
                        .map(|(i, v)| crate::bridge::EquityPoint {
                            date: dates.get(i).unwrap_or(&"2026-01-07").to_string(),
                            value: *v,
                        })
                        .collect();
                    series.push((format!("E{id} / v{ver}"), pts));
                }
            } else if let Some(bridge) = &self.bridge {
                for (id, ver, task) in task_latest.iter().rev() {
                    match bridge.equity_curve(task) {
                        Ok(pts) => series.push((format!("E{id} / v{ver}"), pts)),
                        Err(e) if e.code == nautilus_research_domain::ErrorCode::RunNotReady => {
                            pending = true;
                        }
                        Err(e) => {
                            error = Some(format!("E{id} 净值读取失败：{}", e.message));
                        }
                    }
                }
            }
            self.equity_chart = EquityChartState {
                key,
                series,
                error,
                pending,
            };
        }
        let banner_ids: Option<(usize, usize)> = if demo_latest.len() == 2 {
            Some((demo_latest[1].0, demo_latest[0].0))
        } else if task_latest.len() == 2 {
            Some((task_latest[1].0, task_latest[0].0))
        } else {
            None
        };
        let banner = banner_ids.and_then(|(a, b)| self.workspace.comparison_banner(a, b).ok());
        let empty_experiments = demo_latest.is_empty() && task_latest.is_empty();
        let chart = self.equity_chart.clone();
        let badges: Vec<String> = if !demo_latest.is_empty() {
            demo_latest
                .iter()
                .rev()
                .map(|(id, ver, _)| format!("E{id} / v{ver}"))
                .collect()
        } else {
            task_latest
                .iter()
                .rev()
                .map(|(id, ver, _)| format!("E{id} / v{ver}"))
                .collect()
        };
        panel_with_right(
            ui,
            "净值比较",
            |ui| {
                for b in &badges {
                    theme::tag_ui(ui, b, TagKind::Accent);
                    ui.add_space(6.0);
                }
            },
            |ui| {
                if let Some(text) = &banner {
                    ui.label(note(text.clone()));
                    ui.add_space(8.0);
                }
                if let Some(err) = &chart.error {
                    ui.label(lbl(err.clone(), 11.0, theme::RED));
                }
                if chart.pending {
                    ui.label(note("运行尚未完成，净值曲线待任务终态后可用。"));
                }
                let drawable: Vec<&(String, Vec<crate::bridge::EquityPoint>)> = chart
                    .series
                    .iter()
                    .filter(|(_, pts)| !pts.is_empty())
                    .collect();
                if drawable.is_empty() {
                    if chart.error.is_none() && !chart.pending {
                        ui.label(note(if empty_experiments {
                            "还没有可绘制的实验。运行当前版本后自动绘制最近两次净值曲线。"
                        } else {
                            "净值序列为空：运行产物未含净值行。"
                        }));
                    }
                    return;
                }
                paint_equity_chart(ui, &drawable);
                ui.add_space(6.0);
                ui.label(note(
                    "显示最近两次实验。每个点由现金＋持仓市值计算，悬停查看数值。",
                ));
            },
        );
    }

    // ---- 验证报告 ------------------------------------------------------------
    fn render_validate(&mut self, ui: &mut egui::Ui) {
        if page_header_ex(
            ui,
            "阶段 05 / 检查证据",
            "策略验证",
            "先看证据，再形成有边界的结论。",
            None,
            Some("生成验证报告"),
        ) {
            self.do_make_report();
        }
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
                            .add(theme::small_default_button(&format!(
                                "打开实验 E{} 的事件证据 →",
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
            let items = [
                ("真实行情与历史可见信息检验", "未运行"),
                ("独立样本外区间评估", "未运行"),
                ("参数与成本敏感性检验", "未运行"),
                ("市场适用范围与失效条件评审", "证据不足"),
            ];
            let n = items.len();
            for (i, (name, state)) in items.iter().enumerate() {
                let w = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(w, 42.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let r = ui.max_rect();
                        if i + 1 < n {
                            ui.painter().line_segment(
                                [r.left_bottom(), r.right_bottom()],
                                Stroke::new(1.0, theme::BORDER),
                            );
                        }
                        ui.label(lbl(*name, 11.0, theme::TEXT2));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            theme::tag_ui(ui, *state, TagKind::Warn);
                        });
                    },
                );
            }
        });

        ui.add_space(6.0);
        panel(ui, "四个概念，四种职责", None, |ui| {
            // 原型 `.concepts b`：术语加粗 TEXT2，释义 muted
            for (head, rest) in [
                ("调试", "解释代码为什么这样运行。"),
                ("回测", "观察历史模拟结果。"),
                ("策略验证", "综合证据判断是否满足研究标准。"),
                ("计划核对", "检查这一次调整是否满足当前账户与数据约束。"),
            ] {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.label(
                        egui::RichText::new(head)
                            .font(FontId::proportional(theme::fs(11.0)))
                            .strong()
                            .color(theme::TEXT2),
                    );
                    ui.label(lbl(rest, 11.0, theme::MUTED));
                });
            }
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let plan_btn = theme::primary_button("继续演示交易计划");
            let resp = ui.add_enabled(report_ready(&self.workspace), plan_btn);
            if resp.clicked() {
                let from = self.session.route;
                self.session
                    .navigate(Route::Plan, self.session.scroll_of(from));
            }
            ui.add_space(8.0);
            ui.label(note("仅演示核对流程，不构成正式交易许可"));
        });
    }

    // ---- 交易计划 ------------------------------------------------------------
    fn render_plan_page(&mut self, ui: &mut egui::Ui) {
        let ready = report_ready(&self.workspace);
        page_header_ex(
            ui,
            "阶段 06 / 从策略到计划",
            "交易计划",
            "一次前向核对：现在持有什么，下一步计划调整什么。",
            Some(("人工参考 · 演示", TagKind::Warn)),
            None,
        );
        if !ready {
            Self::warn_banner(
                ui,
                "当前没有可用于计划演示的验证版本。请先完成当前版本的演示检查；正式验证仍为证据不足。",
            );
        }

        let draft = self.workspace.current().plan.clone();
        panel(
            ui,
            "计划输入",
            Some(("独立账户快照", TagKind::Neutral)),
            |ui| {
                two_field_row(
                    ui,
                    |ui| {
                        field_label(ui, "参考数据快照");
                        let ix = self.ai_plan_snapshot.min(1);
                        egui::ComboBox::from_id_salt("plan-snap")
                            .selected_text(PLAN_SNAP_LABELS[ix])
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for i in 0..2 {
                                    ui.selectable_value(
                                        &mut self.ai_plan_snapshot,
                                        i,
                                        PLAN_SNAP_LABELS[i],
                                    );
                                }
                            });
                    },
                    |ui| {
                        field_label(ui, "计划交易日");
                        let mut d = self.ai_plan_trade_date.clone();
                        if ui
                            .add(line_edit(&mut d).desired_width(ui.available_width()))
                            .changed()
                        {
                            self.ai_plan_trade_date = d;
                        }
                    },
                );
                ui.add_space(16.0);
                field_label(ui, "可用现金（元）");
                let mut cash = self.ai_plan_cash.clone();
                if ui
                    .add(line_edit(&mut cash).desired_width(ui.available_width()))
                    .changed()
                {
                    self.ai_plan_cash = cash;
                }
                ui.add_space(16.0);
                ui.columns(3, |cols| {
                    for i in 0..3 {
                        field_label(&mut cols[i], format!("{} · 当前持仓", PLAN_SYMS[i]));
                        let mut q = self.ai_plan_qty[i].clone();
                        if cols[i]
                            .add(line_edit(&mut q).desired_width(cols[i].available_width()))
                            .changed()
                        {
                            self.ai_plan_qty[i] = q;
                        }
                        cols[i].add_space(9.0);
                        field_label(&mut cols[i], "可卖数量");
                        let mut s = self.ai_plan_sell[i].clone();
                        if cols[i]
                            .add(line_edit(&mut s).desired_width(cols[i].available_width()))
                            .changed()
                        {
                            self.ai_plan_sell[i] = s;
                        }
                    }
                });
                ui.add_space(10.0);
                ui.label(note(
                    "计划参考价 A / B / C = 11.00 / 19.50 / 8.50 元，区别于回测期末价格。目标市值以现金＋持仓市值为基数；本轮买入不预支卖出收入。",
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(ready, |ui| {
                        if ui.add(theme::primary_button("生成计划草稿")).clicked() {
                            self.do_ai_plan_generate();
                        }
                    });
                    if let Some(t) = &draft {
                        ui.add_space(8.0);
                        let live_sig = {
                            let cash: f64 =
                                self.ai_plan_cash.trim().parse().unwrap_or(0.0);
                            let snap = PLAN_SNAP_IDS[self.ai_plan_snapshot.min(1)];
                            let rows = crate::bridge::parse_plan_rows(&self.rebuild_ai_plan_json())
                                .map(|(_, _, r)| r)
                                .unwrap_or_default();
                            crate::workspace::plan_signature(
                                t.version_id,
                                snap,
                                self.ai_plan_trade_date.trim(),
                                cash,
                                &rows,
                            )
                        };
                        if live_sig == t.signature {
                            theme::tag_ui(ui, "输入已冻结", TagKind::Accent);
                        } else {
                            theme::tag_ui(ui, "输入已变更 · 需重新生成", TagKind::Warn);
                        }
                    }
                });
            },
        );

        if let Some(t) = &draft {
            ui.add_space(6.0);
            let check_badge = if t.checked
                && self.ai_plan_issues.as_ref().is_some_and(|i| i.is_empty())
            {
                ("演示核对通过", TagKind::Accent)
            } else {
                ("尚未通过核对", TagKind::Warn)
            };
            panel(ui, "持仓调整清单", Some(check_badge), |ui| {
                egui::Grid::new("plan-rows")
                    .num_columns(5)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for h in ["标的", "参考价", "当前 / 可卖", "目标", "调整"] {
                            ui.label(mono(h, 10.0, theme::FAINT));
                        }
                        ui.end_row();
                        for (sym, price, cur, sell, tgt, delta) in &t.rows {
                            ui.label(lbl(sym.clone(), 12.0, theme::TEXT));
                            ui.label(mono(format!("{price:.2}"), 11.0, theme::TEXT2));
                            ui.label(mono(format!("{cur} / {sell}"), 11.0, theme::TEXT2));
                            ui.label(mono(format!("{tgt}"), 11.0, theme::TEXT2));
                            let adj = if *delta > 0 {
                                format!("买入 +{delta}")
                            } else if *delta < 0 {
                                format!("卖出 {}", delta.abs())
                            } else {
                                "保持".into()
                            };
                            ui.label(lbl(
                                adj,
                                11.0,
                                if *delta != 0 {
                                    theme::ACCENT_TEXT
                                } else {
                                    theme::MUTED
                                },
                            ));
                            ui.end_row();
                        }
                    });
                let buys: f64 = t
                    .rows
                    .iter()
                    .filter(|r| r.5 > 0)
                    .map(|r| r.1 * r.5 as f64)
                    .sum();
                let budget = {
                    let p = self.workspace.current();
                    let alloc = p
                        .active_design
                        .and_then(|id| p.designs.get(id - 1))
                        .map(|d| d.allocation)
                        .unwrap_or(1.0);
                    t.total_assets * alloc
                };
                ui.add_space(10.0);
                kv(
                    ui,
                    &[
                        ("账户总资产", format!("{:.2} 元", t.total_assets)),
                        ("目标投入", format!("{:.2} 元", budget)),
                        ("买入含费合计", format!("{:.2} 元", buys)),
                        (
                            "来源",
                            format!("v{} · {} · {}", t.version_id, t.snapshot, t.trade_date),
                        ),
                    ],
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(theme::default_button("核对计划")).clicked() {
                        self.do_ai_plan_check();
                    }
                    ui.add_space(8.0);
                    let export_ok = t.checked
                        && self.ai_plan_issues.as_ref().is_some_and(|i| i.is_empty());
                    ui.add_enabled_ui(export_ok, |ui| {
                        if ui.add(theme::primary_button("确认并导出")).clicked() {
                            self.plan_export_confirm = true;
                        }
                    });
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
                if let Some(csv) = &self.ai_plan_csv {
                    ui.add_space(8.0);
                    ui.label(note("导出内容（含演示标识）："));
                    ui.label(mono(csv, 11.0, theme::TEXT));
                }
            });
        }

        ui.add_space(6.0);
        panel(ui, "计划核对不是回测", None, |ui| {
            ui.label(concepts(
                "回测在历史区间反复模拟策略；这里对一个交易日的具体调整做约束核对。核对通过不能保证成交或盈利。文件始终标注“演示计划”，不产生订单。",
            ));
        });

        // 生产对接面板默认折叠，避免挤占原型「计划输入 / 清单」主路径
        ui.add_space(6.0);
        ui.collapsing("协调器计划（生产对接）", |ui| {
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
            ui.label(field("协调器持仓（空 = 使用最近持有版本）"));
            let mut holdings = self.plan_form.holdings_json.clone();
            if ui
                .add(
                    egui::TextEdit::multiline(&mut holdings)
                        .desired_rows(3)
                        .desired_width(ui.available_width())
                        .font(FontId::monospace(theme::fs(12.0))),
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
        self.poll(PageKey::Plan);
    }

    // ---- 数据中心 ------------------------------------------------------------
    fn render_data(&mut self, ui: &mut egui::Ui) {
        let data_rev = self.workspace.current().data_revision;
        let data_id = format!("SYN-202601-r{data_rev}");
        if page_header_ex(
            ui,
            "资源 / 行情与快照",
            "数据中心",
            "统一管理研究输入，历史实验始终引用原快照。",
            None,
            Some("模拟更新快照"),
        ) {
            self.start_demo_task("模拟更新数据快照", DemoAction::UpdateData);
        }

        // 演示主路径（对齐原型 dataView）
        panel(
            ui,
            "当前合成数据快照",
            Some(("只读演示", TagKind::Neutral)),
            |ui| {
                egui::Grid::new("data_demo_kv")
                    .num_columns(2)
                    .spacing([16.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(field("快照编号"));
                        ui.label(mono(data_id.clone(), 12.0, theme::TEXT));
                        ui.end_row();
                        ui.label(field("样本覆盖"));
                        ui.label(lbl("3 个合成标的 · 2026-01-05～01-07", 12.0, theme::TEXT));
                        ui.end_row();
                        ui.label(field("数据质量"));
                        ui.label(lbl("样本数值完整；非真实行情", 12.0, theme::TEXT));
                        ui.end_row();
                        ui.label(field("复权口径"));
                        ui.label(lbl("未建模公司行为，无真实复权处理", 12.0, theme::TEXT));
                        ui.end_row();
                        ui.label(field("来源"));
                        ui.label(lbl("内置确定性样本，无网络接入", 12.0, theme::TEXT));
                        ui.end_row();
                    });
                ui.add_space(10.0);
                ui.label(note(
                    "模拟更新创建新的快照引用，使当前版本与结论过期。通达信导入、TickFlow 在线更新属于后续生产能力。",
                ));
            },
        );

        ui.add_space(6.0);
        panel(ui, "价格与信号样本", None, |ui| {
            egui::Grid::new("data_fixture_table")
                .striped(true)
                .num_columns(5)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for h in ["代码", "信号收盘", "次日开盘", "期末收盘", "成交额 / 万"] {
                        ui.label(field(h));
                    }
                    ui.end_row();
                    for row in &crate::demo_sim::FIXTURE {
                        ui.label(mono(row.symbol, 11.0, theme::TEXT));
                        ui.label(mono(format!("{:.2}", row.close), 11.0, theme::TEXT));
                        ui.label(mono(format!("{:.2}", row.open), 11.0, theme::TEXT));
                        ui.label(mono(format!("{:.2}", row.marks[2]), 11.0, theme::TEXT));
                        ui.label(mono(format!("{:.0}", row.amount), 11.0, theme::TEXT));
                        ui.end_row();
                    }
                });
        });

        // 协调器导入（次要路径，折叠）
        ui.add_space(8.0);
        egui::CollapsingHeader::new("协调器导入（可选）")
            .default_open(false)
            .show(ui, |ui| {
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
                if !snapshots.is_empty() {
                    ui.add_space(8.0);
                    for (id, as_of, hash) in snapshots {
                        ui.label(mono(
                            format!("{id}  as_of={as_of}  清单 {hash}…"),
                            11.0,
                            theme::TEXT,
                        ));
                    }
                } else if let Some(err) = &self.bridge_error {
                    ui.label(note(err.as_str()));
                }
            });
        self.poll(PageKey::Snapshot);
    }

    // ---- 股票池 --------------------------------------------------------------
    fn render_pool(&mut self, ui: &mut egui::Ui) {
        let pool_rev = self.workspace.current().pool_revision;
        let pool_amount = self.workspace.current().pool_amount;
        let pool_id = format!("U{pool_rev}");
        let markets = ["全部", "沪市", "深市"];
        page_header_ex(
            ui,
            "资源 / 可研究的标的",
            "股票池",
            "查看每个标的的入选依据，保存版本供实验引用。",
            Some((pool_id.as_str(), TagKind::Accent)),
            None,
        );

        // 演示主路径（对齐原型 poolView）
        let mut save_pool = false;
        panel(ui, "筛选规则", None, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.pool_search)
                        .desired_width(200.0)
                        .hint_text("搜索样本代码或名称"),
                );
                ui.add_space(10.0);
                egui::ComboBox::from_id_salt("pool_market")
                    .selected_text(markets[self.pool_market.min(2)])
                    .width(88.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in markets.iter().enumerate() {
                            ui.selectable_value(&mut self.pool_market, i, *name);
                        }
                    });
            });
            ui.add_space(10.0);
            ui.label(field("最低成交额（万元）"));
            let mut amount = format!("{pool_amount:.0}");
            if ui
                .add(egui::TextEdit::singleline(&mut amount).desired_width(120.0))
                .changed()
            {
                if let Ok(v) = amount.parse::<f64>() {
                    self.workspace.current_mut().pool_amount = v;
                }
            }
            ui.add_space(12.0);
            if ui.add(theme::primary_button("保存股票池规则")).clicked() {
                save_pool = true;
            }
            ui.add_space(8.0);
            ui.label(note(
                "搜索只过滤展示；保存采用当前市场与成交额规则。历史实验继续引用当时的规则和成员。",
            ));
        });
        if save_pool {
            let amount = self.workspace.current().pool_amount;
            let market = markets[self.pool_market.min(2)];
            match self.workspace.save_demo_pool(amount, market) {
                Ok((rev, members)) => {
                    self.record(&format!(
                        "股票池 · 已保存 U{rev}，成员 {} 个：{}",
                        members.len(),
                        members.join(", ")
                    ));
                    self.ai_notice = Some("股票池已更新，请保存当前版本后再运行实验。".into());
                }
                Err(e) => {
                    self.ai_notice = Some(e);
                }
            }
        }

        ui.add_space(6.0);
        let q = self.pool_search.trim().to_lowercase();
        let market = markets[self.pool_market.min(2)];
        let amount = self.workspace.current().pool_amount;
        let mut open_stock: Option<usize> = None;
        panel(ui, "候选样本", None, |ui| {
            egui::Grid::new("pool_candidates")
                .striped(true)
                .num_columns(3)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    for h in ["代码 / 市场", "成交额", "入选原因"] {
                        ui.label(field(h));
                    }
                    ui.end_row();
                    let mut any = false;
                    for (idx, row) in crate::demo_sim::FIXTURE.iter().enumerate() {
                        let hit_q = q.is_empty()
                            || row.symbol.to_lowercase().contains(&q)
                            || row.name.to_lowercase().contains(&q);
                        let hit_m = market == "全部" || row.market == market;
                        if !(hit_q && hit_m) {
                            continue;
                        }
                        any = true;
                        let pass = row.amount >= amount;
                        let cell = ui
                            .vertical(|ui| {
                                let r = ui.add(
                                    egui::Label::new(mono(row.symbol, 11.0, theme::TEXT))
                                        .sense(Sense::click()),
                                );
                                ui.label(lbl(
                                    format!("{} · {}", row.market, row.name),
                                    10.0,
                                    theme::MUTED,
                                ));
                                r
                            })
                            .inner;
                        if cell.clicked() {
                            open_stock = Some(idx);
                        }
                        ui.label(mono(format!("{} 万", row.amount as i64), 11.0, theme::TEXT));
                        theme::tag_ui(
                            ui,
                            if pass { "满足流动性" } else { "成交额不足" },
                            if pass { TagKind::Accent } else { TagKind::Warn },
                        );
                        ui.end_row();
                    }
                    if !any {
                        ui.label(note("没有匹配的样本，请调整搜索条件。"));
                        ui.end_row();
                    }
                });
            ui.add_space(8.0);
            ui.label(note("点击样本查看价格、信号与排除原因。"));
        });
        if let Some(idx) = open_stock {
            self.pool_stock_detail = Some(idx);
        }

        // 协调器规则（次要路径）
        ui.add_space(8.0);
        egui::CollapsingHeader::new("协调器规则预览（可选）")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(field("规则 JSON（RuleAST）"));
                let mut rule = self.universe_form.rule_text.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut rule)
                            .desired_rows(4)
                            .desired_width(ui.available_width())
                            .font(FontId::monospace(theme::fs(12.0))),
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
            });
        self.poll(PageKey::Universe);
    }

    // ---- 策略资产 ------------------------------------------------------------
    fn render_library(&mut self, ui: &mut egui::Ui) {
        let versions: Vec<(usize, usize, usize, u64, u64, bool, bool)> = {
            let p = self.workspace.current();
            p.versions
                .iter()
                .map(|v| {
                    (
                        v.id,
                        v.req_id,
                        v.design_id,
                        v.data_revision,
                        v.pool_revision,
                        self.workspace.version_fresh(v.id),
                        p.active_version == Some(v.id),
                    )
                })
                .collect()
        };
        if page_header_ex(
            ui,
            "资源 / 可复用的策略",
            "策略资产",
            "当前项目的策略版本与上游引用。",
            None,
            Some("打开编辑器"),
        ) {
            let from = self.session.route;
            self.session
                .navigate(Route::Develop, self.session.scroll_of(from));
            return;
        }
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
        panel(ui, "版本库", None, |ui| {
            for (id, req, design, data_rev, pool_rev, fresh, active) in &versions {
                let variant = self
                    .workspace
                    .current()
                    .versions
                    .get(id - 1)
                    .map(|v| {
                        if crate::workspace::code_preset_variant(&v.source) == Some(2) {
                            "资金约束修复"
                        } else {
                            "原始示例"
                        }
                    })
                    .unwrap_or("原始示例");
                let state = if *active {
                    "当前选择"
                } else if *fresh {
                    "可用于研究"
                } else {
                    "上游已过期"
                };
                let kind = if *active || *fresh {
                    TagKind::Accent
                } else {
                    TagKind::Warn
                };
                let w = ui.available_width();
                let row = ui.allocate_ui_with_layout(
                    egui::vec2(w, 52.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let r = ui.max_rect();
                        ui.painter().rect(
                            r,
                            CornerRadius::same(7),
                            theme::WHITE_03,
                            Stroke::new(1.0, theme::BORDER_STRONG),
                            egui::StrokeKind::Inside,
                        );
                        ui.add_space(12.0);
                        ui.vertical(|ui| {
                            ui.label(lbl(format!("v{id} · {variant}"), 12.0, theme::TEXT));
                            ui.add_space(5.0);
                            ui.label(mono(
                                format!(
                                    "R{req} / D{design} / SYN-202601-r{data_rev} / U{pool_rev}"
                                ),
                                10.0,
                                theme::MUTED,
                            ));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(12.0);
                            theme::tag_ui(ui, state, kind);
                        });
                    },
                );
                if row.response.interact(Sense::click()).clicked() {
                    self.workspace.current_mut().active_version = Some(*id);
                }
                ui.add_space(6.0);
            }
        });
    }

    // ---- 浮层 ----------------------------------------------------------------
    fn render_windows(&mut self, ctx: &egui::Context) {
        // 运行记录（任务动作、依据和产物；不展示模型内部推理）
        let mut fail_next_click = false;
        let mut retry_click: Option<(&'static str, DemoAction)> = None;
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
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(theme::small_default_button("下次任务模拟失败"))
                        .clicked()
                    {
                        fail_next_click = true;
                    }
                    if let Some((label, action)) = self.demo_retry {
                        ui.add_space(8.0);
                        if ui.add(theme::small_default_button("重试上次任务")).clicked() {
                            retry_click = Some((label, action));
                        }
                    }
                });
            });
        if fail_next_click {
            self.fail_next = true;
            self.ai_notice = Some("下次后台任务将模拟失败，可随后重试".into());
        }
        if let Some((label, action)) = retry_click {
            self.show_logs = false;
            self.start_demo_task(label, action);
        }

        // 股票池样本详情（原型 data-stock modal）
        if let Some(idx) = self.pool_stock_detail {
            if let Some(row) = crate::demo_sim::FIXTURE.get(idx) {
                let amount = self.workspace.current().pool_amount;
                let pass = row.amount >= amount;
                let mut open = true;
                egui::Window::new(format!("{} · {}", row.symbol, row.name))
                    .open(&mut open)
                    .default_width(360.0)
                    .collapsible(false)
                    .show(ctx, |ui| {
                        egui::Grid::new("pool_stock_kv")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(field("市场"));
                                ui.label(lbl(row.market, 12.0, theme::TEXT));
                                ui.end_row();
                                ui.label(field("成交额"));
                                ui.label(lbl(format!("{} 万元", row.amount as i64), 12.0, theme::TEXT));
                                ui.end_row();
                                ui.label(field("示例信号"));
                                ui.label(lbl(
                                    if row.signal { "正向" } else { "无信号" },
                                    12.0,
                                    theme::TEXT,
                                ));
                                ui.end_row();
                                ui.label(field("流动性条件"));
                                ui.label(lbl(
                                    if pass {
                                        "满足当前阈值"
                                    } else {
                                        "不满足当前阈值"
                                    },
                                    12.0,
                                    theme::TEXT,
                                ));
                                ui.end_row();
                            });
                    });
                if !open {
                    self.pool_stock_detail = None;
                }
            } else {
                self.pool_stock_detail = None;
            }
        }

        // 版本只读查看（原型 eventSource：当前事件行前加 ▶）
        if let Some((id, src, highlight)) = self.view_source.clone() {
            let mut open = true;
            let title = match highlight {
                Some(n) => format!("冻结源码 v{id} · 第 {n} 行"),
                None => format!("只读策略 v{id}"),
            };
            egui::Window::new(title)
                .open(&mut open)
                .default_width(560.0)
                .default_height(420.0)
                .show(ctx, |ui| {
                    let body = src
                        .lines()
                        .enumerate()
                        .map(|(i, l)| {
                            let n = i as u32 + 1;
                            let mark = if highlight == Some(n) { "▶ " } else { "  " };
                            format!("{mark}{n:>2}  {l}")
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    ui.label(mono(body, 11.0, theme::TEXT2));
                });
            if !open {
                self.view_source = None;
            }
        }

        // 计划导出二次确认（原型 exportPlan dialog）
        if self.plan_export_confirm {
            let mut open = true;
            egui::Window::new("确认导出演示计划")
                .open(&mut open)
                .collapsible(false)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(lbl(
                        "导出内容标注为演示计划，不构成交易指令；正式策略验证仍为证据不足。",
                        12.0,
                        theme::TEXT2,
                    ));
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.add(theme::primary_button("确认导出")).clicked() {
                            self.plan_export_confirm = false;
                            self.do_ai_plan_export();
                        }
                        ui.add_space(8.0);
                        if ui.add(theme::ghost_button("取消")).clicked() {
                            self.plan_export_confirm = false;
                        }
                    });
                });
            if !open {
                self.plan_export_confirm = false;
            }
        }

        // 需求只读查看（原型 data-req）
        if let Some(id) = self.view_req {
            let req = self
                .workspace
                .current()
                .reqs
                .get(id.saturating_sub(1))
                .cloned();
            let mut open = true;
            egui::Window::new(format!("只读需求 R{id}"))
                .open(&mut open)
                .default_width(520.0)
                .show(ctx, |ui| {
                    if let Some(r) = req {
                        ui.label(field("研究目标与处理逻辑"));
                        ui.label(lbl(&r.text, 12.0, theme::TEXT2));
                        ui.add_space(10.0);
                        ui.label(field("验收标准"));
                        ui.label(lbl(&r.acceptance, 12.0, theme::TEXT2));
                        ui.add_space(10.0);
                        kv(
                            ui,
                            &[
                                ("投入比例", format!("{:.0}%", r.allocation * 100.0)),
                                ("最低成交额", format!("{} 万元", r.min_amount)),
                            ],
                        );
                    } else {
                        ui.label(note("该需求版本不存在。"));
                    }
                });
            if !open {
                self.view_req = None;
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
                                self.push_artifact(
                                    id,
                                    n - 1,
                                    Route::Requirements,
                                    crate::nav::WELCOME_CARD_TITLE,
                                    crate::nav::WELCOME_CARD_DESC,
                                );
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
            // 运行任务终态落定 → 净值曲线缓存失效重取（RD-005）
            if matches!(key, PageKey::Run) {
                self.equity_chart.key = (None, None);
            }
        }
        // 实验任务终态后如需回填收益，由后续协调器指标读取承载（当前诚实留空）
    }

    /// 对话发送：意图路由 → 执行对应动作（对齐原型 submitChat，不只跳转）。
    fn do_ai_send_text(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        let intent = ai::route(&text);
        {
            let p = self.workspace.current_mut();
            p.messages.push(crate::workspace::ChatMessage {
                role: crate::workspace::Role::User,
                text: text.clone(),
            });
        }
        match intent {
            ai::Intent::DraftRequirement => self.apply_requirement_draft(&text),
            ai::Intent::GenerateDesign => {
                self.start_demo_task("生成策略设计", DemoAction::DesignDraft);
            }
            ai::Intent::GenerateCode => {
                self.start_demo_task("生成策略代码", DemoAction::GenerateCode);
            }
            ai::Intent::RunExperiment => self.do_ai_run(),
            ai::Intent::MakeReport => self.do_make_report(),
            ai::Intent::PlanGenerate => self.do_ai_plan_generate(),
            ai::Intent::PlanCheck => self.do_ai_plan_check(),
            ai::Intent::PlanExport => self.plan_export_confirm = true,
            ai::Intent::NewProject => {
                self.new_project_open = true;
                self.append_intent_reply(&text, intent);
            }
            _ => self.append_intent_reply(&text, intent),
        }
        self.chat_pin = true;
    }

    fn append_intent_reply(&mut self, text: &str, intent: ai::Intent) {
        let reply = ai::reply(text);
        let (pid, assistant_index) = {
            let p = self.workspace.current_mut();
            p.messages.push(crate::workspace::ChatMessage {
                role: crate::workspace::Role::Assistant,
                text: reply,
            });
            (p.id, p.messages.len() - 1)
        };
        if let Some(route) = Route::from_intent(intent) {
            let (title, desc) = if intent == ai::Intent::ExplainBoundary {
                ("查看验证边界", "四类活动分别记录")
            } else {
                route.artifact_copy()
            };
            self.push_artifact(pid, assistant_index, route, title, desc);
            let from = self.session.route;
            self.session.navigate(route, self.session.scroll_of(from));
        }
    }

    /// 原型 submitChat：芯片「生成需求草稿」保留默认文案；其它输入写入草稿。
    fn apply_requirement_draft(&mut self, text: &str) {
        let exact = matches!(text.trim(), "生成需求草稿" | "帮我生成需求");
        if !exact {
            self.ai_req_text = text.trim().to_string();
        }
        self.tell_card(
            "需求草稿已整理到工作区。请补充研究范围、参数和验收标准，然后确认需求版本。",
            Some(Route::Requirements),
            Some("研究需求草稿"),
            Some("可直接编辑与确认"),
        );
        let from = self.session.route;
        self.session
            .navigate(Route::Requirements, self.session.scroll_of(from));
    }

    /// 原型 generateDesign：只填草稿，不保存 D。
    fn apply_design_draft_from_req(&mut self) {
        let Some((id, text, alloc, min_amt)) = ({
            let p = self.workspace.current();
            p.active_req.and_then(|rid| {
                p.reqs.get(rid - 1).map(|r| {
                    (r.id, r.text.clone(), r.allocation * 100.0, r.min_amount)
                })
            })
        }) else {
            self.ai_notice = Some("请先确认需求文档".into());
            return;
        };
        self.ai_design_alloc = alloc;
        self.ai_design_min_amount = format!("{min_amt:.0}");
        self.ai_design_note = format!(
            "按需求 R{id}：{text}\n\n处理顺序：可见数据 → 股票池过滤 → 信号形成 → 仓位 → 模拟成交 → 指标。信号日与执行日分离。"
        );
        self.tell_card(
            "设计草稿已生成。两张图共用六个处理节点，请检查参数和说明后保存设计版本。",
            Some(Route::Design),
            Some("设计草稿与处理图"),
            Some("等待保存设计版本"),
        );
        let from = self.session.route;
        self.session
            .navigate(Route::Design, self.session.scroll_of(from));
    }

    /// 追加助手消息 + 运行记录（产物卡可选）。
    fn tell(&mut self, text: impl Into<String>, card: Option<Route>) {
        self.tell_card(text, card, None, None);
    }

    fn tell_card(
        &mut self,
        text: impl Into<String>,
        card: Option<Route>,
        title: Option<&str>,
        desc: Option<&str>,
    ) {
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
            let (dt, dd) = r.artifact_copy();
            self.push_artifact(
                pid,
                idx,
                r,
                title.unwrap_or(dt),
                desc.unwrap_or(dd),
            );
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

    /// 启动离线演示任务（原型 `startTask`，约 850ms）。
    fn start_demo_task(&mut self, label: &'static str, action: DemoAction) {
        if self.demo_task.is_some() {
            self.ai_notice = Some("当前任务正在执行，请等待或取消".into());
            return;
        }
        self.demo_task_seq += 1;
        let stamp = self.workspace.current().revision;
        self.demo_retry = Some((label, action));
        self.demo_task = Some(DemoTask {
            id: self.demo_task_seq,
            label,
            action,
            stamp,
            started: std::time::Instant::now(),
        });
        self.record(&format!("{label} · 已启动"));
    }

    /// 取消离线演示任务。
    fn cancel_demo_task(&mut self) {
        if let Some(t) = self.demo_task.take() {
            self.record(&format!("{} · 已取消", t.label));
            self.tell("任务已取消，未提交新产物。已有版本和实验继续保留。", None);
        }
    }

    /// 推进到期的离线演示任务。
    fn poll_demo_task(&mut self) {
        let Some(task) = self.demo_task.clone() else {
            return;
        };
        if task.started.elapsed() < std::time::Duration::from_millis(850) {
            return;
        }
        // 已被取消或被新任务替换
        if self.demo_task.as_ref().map(|t| t.id) != Some(task.id) {
            return;
        }
        self.demo_task = None;
        if self.fail_next {
            self.fail_next = false;
            self.tell(
                format!(
                    "{}失败：已模拟计算服务不可用。没有提交新产物，可以重试。",
                    task.label
                ),
                None,
            );
            self.record("失败模拟，无结果提交");
            return;
        }
        if self.workspace.current().revision != task.stamp {
            self.tell(
                "输入版本已变化，本次任务未提交结果。请以当前版本重试。",
                None,
            );
            return;
        }
        match task.action {
            DemoAction::DesignDraft => {
                self.apply_design_draft_from_req();
                self.demo_retry = None;
            }
            DemoAction::GenerateCode => {
                self.do_generate_code();
                self.demo_retry = None;
            }
            DemoAction::UpdateData => {
                let rev = self.workspace.update_demo_data();
                self.record(&format!(
                    "数据中心 · 已更新合成快照 SYN-202601-r{rev}；当前版本与结论已过期"
                ));
                self.tell_card(
                    format!(
                        "新合成快照 SYN-202601-r{rev} 已建立。当前版本和结论需要重新确认，历史实验不变。"
                    ),
                    Some(Route::Data),
                    Some("数据快照已更新"),
                    Some("原型更新，不连接行情服务"),
                );
                self.demo_retry = None;
            }
        }
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
        // 预置示例：离线 compute 已写入 demo，对齐原型主路径（不依赖数据中心）
        if let Some(demo) = self
            .workspace
            .current()
            .experiments
            .iter()
            .find(|e| e.id == exp_id)
            .and_then(|e| e.demo.clone())
        {
            let tip = if demo.rejected > 0 {
                "订单因资金不足被拒绝，可进入事件调试定位。"
            } else {
                "计算结果已保存，接下来检查验证证据。"
            };
            self.tell(
                format!(
                    "实验 E{exp_id} 完成：期末净值 {:.2}，收益 {:.2}%。{tip}",
                    demo.final_nav, demo.return_pct
                ),
                Some(Route::Experiments),
            );
            self.record(&format!("运行实验 E{exp_id} · 合成演示完成"));
            return;
        }
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
            Ok(vid) => {
                let (demo_pass, run_id) = self
                    .workspace
                    .current()
                    .report
                    .as_ref()
                    .map(|r| (r.demo_pass(), r.run_id))
                    .unwrap_or((false, 0));
                let demo_label = if demo_pass { "通过" } else { "失败" };
                let tail = if demo_pass {
                    "可以继续演示计划生成与账户核对。"
                } else {
                    "请修复问题并运行新版本。"
                };
                self.tell_card(
                    format!(
                        "验证报告已生成：演示检查{demo_label}；正式策略验证仍为证据不足。真实数据、样本外与参数稳健性未运行。{tail}"
                    ),
                    Some(Route::Validate),
                    Some(&format!("验证报告 · v{vid}")),
                    Some(&format!("关联实验 E{run_id}")),
                );
            }
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
                let desc = format!("需求 R{id} · 等待设计");
                self.tell_card(
                    format!("需求 R{id} 已确认。下一步生成设计说明与两张处理图。旧实验保留，新研究将使用此需求版本。"),
                    Some(Route::Design),
                    Some("生成策略设计"),
                    Some(desc.as_str()),
                );
            }
            Err(e) => self.ai_notice = Some(e),
        }
    }

    /// 导出当前需求草稿为 Markdown（原型「下载文档」；写到工作区目录）。
    fn export_requirement_draft(&mut self) {
        let body = format!(
            "# 研究需求\n\n{}\n\n## 验收标准\n\n{}\n\n- 目标投入比例：{:.0}%\n- 最低成交额：{} 万元\n",
            self.ai_req_text,
            self.ai_req_acceptance,
            self.ai_req_alloc,
            self.ai_req_min_amount
        );
        let dir = std::env::var_os("RESEARCH_WORKSPACE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("research-workspace"));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("requirement-draft.md");
        match std::fs::write(&path, body) {
            Ok(()) => self.record(&format!("已写出需求草稿 {}", path.display())),
            Err(e) => self.ai_notice = Some(format!("写出需求草稿失败：{e}")),
        }
    }

    fn export_design_draft(&mut self) {
        let body = format!(
            "# 策略设计草稿\n\n投入比例：{:.0}%\n最低成交额：{} 万元\n\n{}\n",
            self.ai_design_alloc, self.ai_design_min_amount, self.ai_design_note
        );
        let dir = std::env::var_os("RESEARCH_WORKSPACE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("research-workspace"));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("design-draft.md");
        match std::fs::write(&path, body) {
            Ok(()) => self.record(&format!("已写出设计草稿 {}", path.display())),
            Err(e) => self.ai_notice = Some(format!("写出设计草稿失败：{e}")),
        }
    }

    /// 导出当前项目实验列表（原型「导出 JSON」；无逐步事件时仅任务级字段）。
    fn export_experiments_json(&mut self) {
        let rows: Vec<(usize, usize, u64, Option<String>, Option<String>)> = self
            .workspace
            .current()
            .experiments
            .iter()
            .map(|e| {
                (
                    e.id,
                    e.version_id,
                    e.stamp,
                    e.task_id.clone(),
                    e.total_return.clone(),
                )
            })
            .collect();
        let n = rows.len();
        let mut body = String::from("[\n");
        for (i, (id, ver, stamp, task_id, ret)) in rows.into_iter().enumerate() {
            if i > 0 {
                body.push_str(",\n");
            }
            let task = task_id
                .as_deref()
                .map(|t| format!("\"{t}\""))
                .unwrap_or_else(|| "null".into());
            let ret_json = ret
                .as_deref()
                .map(|r| format!("\"{r}\""))
                .unwrap_or_else(|| "null".into());
            body.push_str(&format!(
                "  {{\"id\":{id},\"version_id\":{ver},\"stamp\":{stamp},\"task_id\":{task},\"total_return\":{ret_json}}}"
            ));
        }
        body.push_str("\n]\n");
        let dir = std::env::var_os("RESEARCH_WORKSPACE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("research-workspace"));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("experiments.json");
        match std::fs::write(&path, body) {
            Ok(()) => {
                self.record(&format!("已导出实验 JSON {}", path.display()));
                self.tell(format!("已导出 {n} 条实验到 {}", path.display()), None);
            }
            Err(e) => self.ai_notice = Some(format!("导出实验 JSON 失败：{e}")),
        }
    }

    fn do_generate_design(&mut self) {
        let note_text = self.ai_design_note.clone();
        let alloc = self.ai_design_alloc;
        let min_amount = self
            .ai_design_min_amount
            .parse::<f64>()
            .unwrap_or(f64::NAN);
        match self
            .workspace
            .generate_design(note_text, alloc, min_amount)
        {
            Ok(id) => {
                let req = self.workspace.current().active_req.unwrap_or_default();
                let desc = format!("设计 D{id} · 待生成代码");
                self.tell_card(
                    format!("设计 D{id} 已保存，绑定 R{req}。流程图、时序图与代码将引用相同的节点和参数。"),
                    Some(Route::Develop),
                    Some("开始策略开发"),
                    Some(desc.as_str()),
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

    fn do_check_code(&mut self) {
        let p = self.workspace.current();
        let design_ok = p
            .active_req
            .zip(p.active_design)
            .is_some_and(|(r, d)| p.designs.get(d - 1).is_some_and(|x| x.req_id == r));
        if !design_ok {
            self.ai_notice = Some("当前设计缺失或已过期，请重新保存设计".into());
            return;
        }
        match crate::workspace::code_preset_variant(&self.ai_code_source) {
            Some(_) => {
                self.ai_code_checked = Some(self.ai_code_source.clone());
                self.ai_notice = None;
                self.record("示例映射检查通过 · 未执行 Python");
            }
            None => {
                self.ai_code_checked = None;
                self.ai_notice = Some(
                    "自定义源码已保留，但离线模拟器仅支持两份完整预置示例，不能执行此源码。"
                        .into(),
                );
            }
        }
    }

    fn do_generate_code(&mut self) {
        let p = self.workspace.current();
        let design_ok = p
            .active_req
            .zip(p.active_design)
            .is_some_and(|(r, d)| p.designs.get(d - 1).is_some_and(|x| x.req_id == r));
        if !design_ok {
            self.ai_notice = Some("请先保存与当前需求匹配的设计".into());
            return;
        }
        self.ai_code_source = crate::workspace::DRAFT_CODE_ORIGINAL.into();
        self.ai_code_checked = None;
        self.show_code_diff = false;
        self.tell(
            "已生成可编辑的策略示例。先检查并保存 v1，再运行实验观察资金约束。此示例不在桌面中执行 Python。",
            Some(Route::Develop),
        );
    }

    fn export_strategy_source(&mut self) {
        let dir = std::env::var_os("RESEARCH_WORKSPACE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("research-workspace"));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("策略示例.py");
        match std::fs::write(&path, &self.ai_code_source) {
            Ok(()) => self.record(&format!("已写出 {}", path.display())),
            Err(e) => self.ai_notice = Some(format!("写出策略示例失败：{e}")),
        }
    }

    // ---- 计划桥（S20）
    fn rebuild_ai_plan_json(&self) -> String {
        let cash: f64 = self.ai_plan_cash.trim().parse().unwrap_or(0.0);
        let mut holdings = [0_i64; 3];
        let mut sellable = [0_i64; 3];
        for i in 0..3 {
            holdings[i] = self.ai_plan_qty[i].trim().parse().unwrap_or(0);
            sellable[i] = self.ai_plan_sell[i].trim().parse().unwrap_or(holdings[i]);
        }
        let market: f64 = (0..3)
            .map(|i| PLAN_PRICES[i] * holdings[i] as f64)
            .sum();
        let total = cash + market;
        let (alloc, min_amount) = {
            let p = self.workspace.current();
            p.active_design
                .and_then(|id| p.designs.get(id - 1))
                .map(|d| (d.allocation, d.min_amount))
                .unwrap_or((1.0, 1000.0))
        };
        let budget = total * alloc;
        let has_signal = crate::demo_sim::DEFAULT_POOL.contains(&"SYN-A")
            && crate::demo_sim::FIXTURE[0].amount >= min_amount;
        let mut target = if has_signal {
            ((budget / PLAN_PRICES[0] / 100.0).floor() * 100.0).max(0.0) as i64
        } else {
            0
        };
        if target > holdings[0] && target as f64 * PLAN_PRICES[0] + 5.0 > budget {
            target = (target - 100).max(0);
        }
        let targets = [target, 0, 0];
        let mut positions = Vec::new();
        for i in 0..3 {
            positions.push(serde_json::json!({
                "instrument_id": PLAN_SYMS[i],
                "price": PLAN_PRICES[i],
                "quantity": holdings[i],
                "sellable_quantity": sellable[i],
                "target_quantity": targets[i],
            }));
        }
        serde_json::json!({
            "cash_cny": format!("{cash:.0}"),
            "total_assets_cny": format!("{total:.2}"),
            "positions": positions,
        })
        .to_string()
    }

    fn do_ai_plan_generate(&mut self) {
        self.ai_plan_json = self.rebuild_ai_plan_json();
        let snapshot = PLAN_SNAP_IDS[self.ai_plan_snapshot.min(1)].to_string();
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
            .make_plan(snapshot, date, cash, total, rows)
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
        let Ok((cash, _total, rows)) = crate::bridge::parse_plan_rows(&self.rebuild_ai_plan_json()) else {
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

/// 协调器收益字段 → 百分比文案（`0.0625` 或 `6.25%`）。
fn format_return_pct(raw: &str) -> String {
    let t = raw.trim().trim_end_matches('%');
    match t.parse::<f64>() {
        Ok(v) => {
            let pct = if raw.contains('%') {
                v
            } else if v.abs() <= 2.0 {
                v * 100.0
            } else {
                v
            };
            format!("{pct:.2}%")
        }
        Err(_) => raw.to_string(),
    }
}

/// 以初始 10,000 元把收益折成期末净值。
fn nav_from_return(raw: &str) -> Option<f64> {
    let t = raw.trim().trim_end_matches('%');
    let v = t.parse::<f64>().ok()?;
    let ratio = if raw.contains('%') {
        v / 100.0
    } else if v.abs() <= 2.0 {
        v
    } else {
        v / 100.0
    };
    Some(10_000.0 * (1.0 + ratio))
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
        .font(FontId::proportional(theme::fs(size)))
        .color(color)
}

/// 等宽文字（数字与代码）。
fn mono(text: impl Into<String>, size: f32, color: Color32) -> egui::RichText {
    egui::RichText::new(text.into())
        .font(FontId::monospace(theme::fs(size)))
        .color(color)
}

/// 表单字段标签。
fn field(text: impl Into<String>) -> egui::RichText {
    lbl(text, 12.0, theme::TEXT2)
}

fn field_label(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(field(text));
    ui.add_space(7.0);
}

fn line_edit(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(Margin::same(10))
        .font(FontId::proportional(theme::fs(13.0)))
}

fn area_edit(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text)
        .margin(Margin::same(10))
        .font(FontId::proportional(theme::fs(13.0)))
}

/// 说明文字。
fn note(text: impl Into<String>) -> egui::RichText {
    lbl(text, 11.0, theme::MUTED)
}

/// 概念文字。
fn concepts(text: impl Into<String>) -> egui::RichText {
    lbl(text, 11.0, theme::MUTED)
}

/// tab 小按钮（原型 `small` / 选中 `small primary`）。
fn tab_button(text: &str, active: bool) -> egui::Button<'_> {
    if active {
        theme::small_primary_button(text.to_string())
    } else {
        theme::small_default_button(text.to_string())
    }
}

/// 原型 `.grid2`：舒适档两列顶对齐；窄窗（≤1200）或内容区过窄时单列堆叠。
/// 用 `horizontal_top` + 固定列宽，避免 `columns` 在嵌套面板里错位。
fn two_field_row(
    ui: &mut egui::Ui,
    left: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    let win_w = ui.ctx().content_rect().width();
    let gap = 14.0;
    let avail = ui.available_width();
    let stacked = layout::plan_for_width(win_w) != LayoutPlan::Comfortable || avail < 360.0;
    if stacked {
        left(ui);
        ui.add_space(16.0);
        right(ui);
        return;
    }
    let col_w = ((avail - gap) * 0.5).max(120.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_width(col_w);
                ui.set_max_width(col_w);
                left(ui);
            },
        );
        ui.add_space(gap);
        ui.allocate_ui_with_layout(
            egui::vec2(col_w, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_width(col_w);
                ui.set_max_width(col_w);
                right(ui);
            },
        );
    });
}

/// 页头：eyebrow + 标题 + 副标题。
fn page_header(ui: &mut egui::Ui, eyebrow: &str, title: &str, subtitle: &str) {
    let _ = page_header_ex(ui, eyebrow, title, subtitle, None, None);
}

/// 页头（可选右侧徽章 / 主按钮，原型 `.page-title` 右上 action）。
fn page_header_ex(
    ui: &mut egui::Ui,
    eyebrow: &str,
    title: &str,
    subtitle: &str,
    badge: Option<(&str, TagKind)>,
    action: Option<&str>,
) -> bool {
    let mut clicked = false;
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        if let Some(label) = action {
            if ui.add(theme::primary_button(label)).clicked() {
                clicked = true;
            }
            ui.add_space(8.0);
        }
        if let Some((t, k)) = badge {
            theme::tag_ui(ui, t, k);
            ui.add_space(8.0);
        }
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            ui.set_width(ui.available_width());
            theme::eyebrow_label(ui, eyebrow);
            ui.add_space(10.0);
            // 原型 h1：24px、letter-spacing -0.8、字重约 580
            ui.label(
                egui::RichText::new(title)
                    .font(FontId::proportional(theme::fs(24.0)))
                    .extra_letter_spacing(-0.8)
                    .color(theme::TEXT),
            );
            ui.add_space(8.0);
            ui.label(lbl(subtitle, 11.0, theme::MUTED));
        });
    });
    ui.add_space(20.0);
    clicked
}

/// 玻璃面板：标题头（含徽章）+ 内边距内容。
fn panel(
    ui: &mut egui::Ui,
    title: &str,
    badge: Option<(&str, TagKind)>,
    body: impl FnOnce(&mut egui::Ui),
) {
    let shown = Frame::NONE
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
    theme::paint_drop_shadow(ui.painter(), shown.response.rect, theme::CARD_ROUNDING as f32);
    theme::paint_inset_top(ui.painter(), shown.response.rect, 12.0);
    ui.add_space(12.0);
}

/// 玻璃面板：内容区无内边距（原型研究路径 `.journey-row` 撑满 panel）。
fn panel_flush(
    ui: &mut egui::Ui,
    title: &str,
    badge: Option<(&str, TagKind)>,
    body: impl FnOnce(&mut egui::Ui),
) {
    let shown = Frame::NONE
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
            body(ui);
        });
    theme::paint_drop_shadow(ui.painter(), shown.response.rect, theme::CARD_ROUNDING as f32);
    theme::paint_inset_top(ui.painter(), shown.response.rect, 12.0);
    ui.add_space(15.0);
}

/// 玻璃面板：标题在左、右侧自定义操作（原型 `.panel-head` 流程图/时序图按钮）。
fn panel_with_right(
    ui: &mut egui::Ui,
    title: &str,
    right: impl FnOnce(&mut egui::Ui),
    body: impl FnOnce(&mut egui::Ui),
) {
    let shown = Frame::NONE
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
                        ui.with_layout(Layout::right_to_left(Align::Center), right);
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
    theme::paint_drop_shadow(ui.painter(), shown.response.rect, theme::CARD_ROUNDING as f32);
    theme::paint_inset_top(ui.painter(), shown.response.rect, 12.0);
    ui.add_space(12.0);
}

/// 原型 `.metric-grid`：等宽列、暗玻璃底、三段纵向文字（避免非法预乘白底）。
fn metric_grid(ui: &mut egui::Ui, items: &[(&str, String, String)], slim_first_only: bool) {
    let n = if slim_first_only {
        1.min(items.len())
    } else {
        items.len()
    };
    if n == 0 {
        return;
    }
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.columns(n, |cols| {
            for (i, (top, value, bottom)) in items.iter().take(n).enumerate() {
                Frame::NONE
                    .fill(theme::WHITE_03)
                    .corner_radius(CornerRadius::same(10))
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .inner_margin(Margin::same(17))
                    .show(&mut cols[i], |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.label(lbl(*top, 10.0, theme::MUTED));
                            ui.add_space(10.0);
                            // 原型 `.metric strong`：mono 23、letter-spacing -0.7
                            ui.label(
                                egui::RichText::new(value.clone())
                                    .font(FontId::monospace(theme::fs(23.0)))
                                    .extra_letter_spacing(-0.7)
                                    .color(theme::TEXT),
                            );
                            ui.add_space(5.0);
                            ui.label(lbl(bottom.clone(), 10.0, theme::MUTED));
                        });
                    });
            }
        });
    });
}

/// 原型 `flowSvg`：660×240 视图盒等比落到可用宽度，2×3 节点 + 水平箭头 + 跨日折线。
fn paint_flow_diagram(ui: &mut egui::Ui, selected: usize) -> Option<usize> {
    let w = ui.available_width().max(1.0);
    let scale = (w / 660.0).clamp(0.55, 1.35);
    let h = 240.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(sz(w, h), Sense::click());
    let painter = ui.painter();
    let mut hit = None;
    let mut boxes = [egui::Rect::from_min_max(Pos2::ZERO, Pos2::ZERO); 6];
    for i in 0..6 {
        let nx = 25.0 + (i % 3) as f32 * 218.0;
        let ny = if i < 3 { 28.0 } else { 150.0 };
        let r = egui::Rect::from_min_size(
            Pos2::new(rect.left() + nx * scale, rect.top() + ny * scale),
            egui::vec2(173.0 * scale, 62.0 * scale),
        );
        boxes[i] = r;
        let sel = i == selected;
        painter.rect(
            r,
            CornerRadius::same(9),
            Color32::from_rgb(0x14, 0x18, 0x16),
            Stroke::new(
                1.0,
                if sel {
                    theme::ACCENT
                } else {
                    Color32::from_rgb(0x24, 0x3B, 0x2E)
                },
            ),
            egui::StrokeKind::Inside,
        );
        let (_, name, sub, _, _) = NODES[i];
        painter.text(
            r.min + egui::vec2(12.0 * scale, 14.0 * scale),
            egui::Align2::LEFT_TOP,
            format!("0{}　{name}", i + 1),
            FontId::proportional((11.0 * scale).max(9.0)),
            theme::TEXT,
        );
        painter.text(
            r.min + egui::vec2(12.0 * scale, 34.0 * scale),
            egui::Align2::LEFT_TOP,
            sub,
            FontId::proportional((9.0 * scale).max(8.0)),
            Color32::from_rgb(0x6B, 0x7F, 0x74),
        );
        if i % 3 < 2 {
            let y = r.center().y;
            let x1 = r.right();
            let x2 = r.right() + 36.0 * scale;
            paint_seq_arrow(&painter, x1, x2, y, scale);
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if r.contains(pos) {
                    hit = Some(i);
                }
            }
        }
    }
    let a = boxes[2].center();
    let b = boxes[3].center();
    let mid_y = (boxes[2].bottom() + boxes[3].top()) * 0.5;
    let stroke = Stroke::new(1.0, theme::ACCENT);
    painter.line_segment(
        [Pos2::new(a.x, boxes[2].bottom()), Pos2::new(a.x, mid_y)],
        stroke,
    );
    painter.line_segment([Pos2::new(a.x, mid_y), Pos2::new(b.x, mid_y)], stroke);
    painter.line_segment(
        [Pos2::new(b.x, mid_y), Pos2::new(b.x, boxes[3].top())],
        stroke,
    );
    painter.text(
        Pos2::new((a.x + b.x) * 0.5, mid_y - 10.0 * scale),
        egui::Align2::CENTER_BOTTOM,
        "跨交易日 · 信号与执行分离",
        FontId::proportional((9.0 * scale).max(8.0)),
        theme::MUTED,
    );
    hit
}

/// 原型 `seqhead`：实心绿三角箭头（fill `#22c55e`），水平消息线接生命线。
fn paint_seq_arrow(painter: &egui::Painter, x1: f32, x2: f32, y: f32, scale: f32) {
    let head_w = 7.0 * scale;
    let head_h = 3.5 * scale;
    let tip = x2.max(x1 + head_w + 1.0);
    let shaft = tip - head_w;
    painter.line_segment(
        [Pos2::new(x1, y), Pos2::new(shaft, y)],
        Stroke::new(1.25, theme::ACCENT),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(tip, y),
            Pos2::new(shaft, y - head_h),
            Pos2::new(shaft, y + head_h),
        ],
        theme::ACCENT,
        Stroke::NONE,
    ));
}

/// 原型 `sequenceSvg`：viewBox 660×350。
/// 六列节点头 + `#2f5240` 虚线生命线 + 信号日淡绿色带 + 执行日琥珀标题 + 五条消息。
fn paint_sequence_diagram(ui: &mut egui::Ui, selected: usize) -> Option<usize> {
    let w = ui.available_width().max(1.0);
    let scale = (w / 660.0).clamp(0.72, 1.6);
    let h = 350.0 * scale;
    ui.set_min_height(h);
    let (rect, resp) = ui.allocate_exact_size(sz(w, h), Sense::click());
    let painter = ui.painter();
    let sx = |x: f32| rect.left() + x * scale;
    let sy = |y: f32| rect.top() + y * scale;
    let mut hit = None;
    let node_fill = Color32::from_rgb(0x14, 0x18, 0x16);
    let node_hover = Color32::from_rgb(0x16, 0x24, 0x1B);
    let node_stroke = Color32::from_rgb(0x24, 0x3B, 0x2E);
    let life = Color32::from_rgb(0x2F, 0x52, 0x40);
    let diagram_text = Color32::from_rgb(0xB8, 0xCF, 0xC0);
    let pointer = resp.hover_pos();
    let mut headers = [egui::Rect::from_min_max(Pos2::ZERO, Pos2::ZERO); 6];
    for i in 0..6 {
        let hdr = egui::Rect::from_min_size(
            Pos2::new(sx(8.0 + i as f32 * 109.0), sy(10.0)),
            egui::vec2(98.0 * scale, 34.0 * scale),
        );
        headers[i] = hdr;
        let hovering = pointer.is_some_and(|p| hdr.contains(p));
        painter.rect(
            hdr,
            CornerRadius::same((5.0 * scale).round() as u8),
            if hovering { node_hover } else { node_fill },
            Stroke::new(
                1.0,
                if i == selected {
                    theme::ACCENT
                } else if hovering {
                    theme::ACCENT_TEXT
                } else {
                    node_stroke
                },
            ),
            egui::StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(hdr.center().x, sy(32.0)),
            egui::Align2::CENTER_BOTTOM,
            NODES[i].1,
            FontId::proportional((11.0 * scale).clamp(9.0, 13.0)),
            diagram_text,
        );
        let lx = sx(57.0 + i as f32 * 109.0);
        let mut y = sy(44.0);
        let y_end = sy(333.0);
        let on = 3.0 * scale;
        let gap = 5.0 * scale;
        let dash = Stroke::new(1.0, life);
        while y < y_end {
            let y2 = (y + on).min(y_end);
            painter.line_segment([Pos2::new(lx, y), Pos2::new(lx, y2)], dash);
            y += on + gap;
        }
    }
    let band = egui::Rect::from_min_size(
        Pos2::new(sx(5.0), sy(57.0)),
        egui::vec2(650.0 * scale, 115.0 * scale),
    );
    painter.rect(
        band,
        CornerRadius::same((6.0 * scale).round() as u8),
        Color32::from_rgba_unmultiplied(34, 197, 94, 8),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(34, 197, 94, 38)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        Pos2::new(sx(16.0), sy(75.0)),
        egui::Align2::LEFT_BOTTOM,
        "2026-01-05 · 收盘形成信号",
        FontId::proportional((10.0 * scale).clamp(9.0, 12.0)),
        diagram_text,
    );
    painter.text(
        Pos2::new(sx(16.0), sy(192.0)),
        egui::Align2::LEFT_BOTTOM,
        "2026-01-06 · 开盘执行（此时才读取开盘价）",
        FontId::proportional((10.0 * scale).clamp(9.0, 12.0)),
        theme::AMBER,
    );
    const MSGS: [&str; 5] = [
        "① 可见行情",
        "② 入选样本",
        "③ 目标比例",
        "④ 资金约束",
        "⑤ 成交回报",
    ];
    for i in 0..5 {
        let yv = if i < 2 {
            103.0 + i as f32 * 43.0
        } else {
            217.0 + (i as f32 - 2.0) * 43.0
        };
        let x1 = sx(57.0 + i as f32 * 109.0);
        let x2 = sx(57.0 + (i as f32 + 1.0) * 109.0 - 4.0);
        paint_seq_arrow(&painter, x1, x2, sy(yv), scale);
        painter.text(
            Pos2::new(sx(60.0 + i as f32 * 109.0), sy(yv - 8.0)),
            egui::Align2::LEFT_BOTTOM,
            MSGS[i],
            FontId::proportional((10.0 * scale).clamp(9.0, 12.0)),
            diagram_text,
        );
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            for (i, hdr) in headers.iter().enumerate() {
                let col = egui::Rect::from_min_max(
                    Pos2::new(hdr.left(), rect.top()),
                    Pos2::new(hdr.right(), rect.bottom()),
                );
                if col.contains(pos) {
                    hit = Some(i);
                    break;
                }
            }
        }
    }
    hit
}

/// 空态面板（可选主按钮；返回按钮是否被点击）。
fn empty_panel(ui: &mut egui::Ui, title: &str, desc: &str, action: Option<&str>) -> bool {
    let mut clicked = false;
    let shown = Frame::NONE
        .fill(theme::GLASS)
        .corner_radius(CornerRadius::same(theme::CARD_ROUNDING))
        .stroke(Stroke::new(1.0, theme::BORDER))
        .inner_margin(Margin {
            left: 22,
            right: 22,
            top: 42,
            bottom: 42,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical_centered(|ui| {
                let (icon_rect, _) =
                    ui.allocate_exact_size(egui::Vec2::splat(30.0), Sense::hover());
                icons::paint_icon(
                    ui.painter(),
                    icon_rect,
                    icons::IconKind::Flow,
                    Color32::from_rgb(0x3D, 0x5C, 0x4A),
                );
                ui.add_space(15.0);
                ui.label(lbl(title, 15.0, theme::TEXT));
                ui.add_space(12.0);
                ui.label(lbl(desc, 12.0, theme::MUTED));
                if let Some(a) = action {
                    ui.add_space(19.0);
                    if ui.add(theme::primary_button(a)).clicked() {
                        clicked = true;
                    }
                }
            });
        });
    theme::paint_drop_shadow(ui.painter(), shown.response.rect, theme::CARD_ROUNDING as f32);
    theme::paint_inset_top(ui.painter(), shown.response.rect, 12.0);
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

/// 原型 `equityChart()`：640×240 SVG 等比落到可用宽——淡网格、绿渐变面积、圆点、三日期轴。
fn paint_equity_chart(
    ui: &mut egui::Ui,
    drawable: &[&(String, Vec<crate::bridge::EquityPoint>)],
) {
    let w = ui.available_width().max(1.0);
    let scale = (w / 640.0).clamp(0.65, 1.35);
    let h = 240.0 * scale;
    let (rect, resp) = ui.allocate_exact_size(sz(w, h), Sense::hover());
    let painter = ui.painter();
    let mx = |x: f32| rect.left() + x * scale;
    let my = |y: f32| rect.top() + y * scale;

    let all: Vec<f64> = drawable
        .iter()
        .flat_map(|(_, pts)| pts.iter().map(|p| p.value))
        .collect();
    let low = all.iter().copied().fold(10_000.0_f64, f64::min) - 100.0;
    let high = all.iter().copied().fold(10_100.0_f64, f64::max) + 100.0;
    let span = (high - low).max(1.0);
    let y_of = |v: f64| my(194.0 - ((v - low) / span) as f32 * 160.0);

    let grid = Color32::from_rgba_unmultiplied(255, 255, 255, 18);
    for i in 0..4 {
        let val = low + span * i as f64 / 3.0;
        let yy = y_of(val);
        painter.line_segment(
            [Pos2::new(mx(60.0), yy), Pos2::new(mx(610.0), yy)],
            Stroke::new(1.0, grid),
        );
        painter.text(
            Pos2::new(mx(4.0), yy),
            egui::Align2::LEFT_CENTER,
            format!("{:.0}", val),
            FontId::proportional((10.0 * scale).max(8.5)),
            theme::FAINT,
        );
    }

    let x_at = |j: usize, n: usize| -> f32 {
        if n <= 1 {
            mx(70.0)
        } else {
            mx(70.0 + j as f32 * 530.0 / (n as f32 - 1.0))
        }
    };

    let mut hover_tip: Option<String> = None;
    let pointer = resp.hover_pos();

    for (si, (label, pts)) in drawable.iter().enumerate() {
        if pts.is_empty() {
            continue;
        }
        let latest = si + 1 == drawable.len();
        let color = if latest {
            theme::ACCENT_TEXT
        } else {
            Color32::from_rgb(0x4B, 0x55, 0x63)
        };
        let n = pts.len();
        let xs: Vec<f32> = (0..n).map(|j| x_at(j, n)).collect();
        let ys: Vec<f32> = pts.iter().map(|p| y_of(p.value)).collect();

        if latest && n >= 2 {
            let base_y = my(199.0);
            let top_c = Color32::from_rgba_unmultiplied(34, 197, 94, 41);
            let bot_c = Color32::from_rgba_unmultiplied(34, 197, 94, 0);
            let mut mesh = egui::Mesh::default();
            for j in 0..n - 1 {
                let i0 = mesh.vertices.len() as u32;
                mesh.colored_vertex(Pos2::new(xs[j], ys[j]), top_c);
                mesh.colored_vertex(Pos2::new(xs[j + 1], ys[j + 1]), top_c);
                mesh.colored_vertex(Pos2::new(xs[j + 1], base_y), bot_c);
                mesh.colored_vertex(Pos2::new(xs[j], base_y), bot_c);
                mesh.add_triangle(i0, i0 + 1, i0 + 2);
                mesh.add_triangle(i0, i0 + 2, i0 + 3);
            }
            painter.add(egui::Shape::mesh(mesh));
        }

        let line: Vec<Pos2> = xs
            .iter()
            .zip(ys.iter())
            .map(|(x, y)| Pos2::new(*x, *y))
            .collect();
        painter.add(egui::Shape::line(
            line,
            Stroke::new(2.5 * scale.max(0.85), color),
        ));
        for j in 0..n {
            let c = Pos2::new(xs[j], ys[j]);
            painter.circle_filled(c, 4.0 * scale.max(0.85), color);
            if let Some(pos) = pointer {
                if c.distance(pos) < 10.0 * scale.max(0.9) {
                    hover_tip = Some(format!(
                        "{label} · {} · 净值 {:.2}",
                        pts[j].date, pts[j].value
                    ));
                }
            }
        }
    }

    // X 轴：优先用最近序列日期，缺省对齐原型「信号日 / 执行日 / 期末」
    let axis_suffix = ["信号日", "执行日", "期末"];
    if let Some((_, pts)) = drawable.last() {
        let n = pts.len();
        for (j, p) in pts.iter().enumerate() {
            let raw = if p.date.len() >= 5 {
                &p.date[p.date.len().saturating_sub(5)..]
            } else {
                p.date.as_str()
            };
            let suffix = axis_suffix.get(j).copied().unwrap_or("");
            let text = if suffix.is_empty() {
                raw.to_string()
            } else {
                format!("{raw} {suffix}")
            };
            let tx = if n == 3 {
                mx([60.0_f32, 302.0, 545.0][j])
            } else {
                x_at(j, n)
            };
            painter.text(
                Pos2::new(tx, my(226.0)),
                egui::Align2::LEFT_CENTER,
                text,
                FontId::proportional((10.0 * scale).max(8.5)),
                theme::FAINT,
            );
        }
    }

    if let Some(tip) = hover_tip {
        resp.on_hover_text(tip);
    }
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

        w.generate_design_from_req("EMA 双均线").unwrap();
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
