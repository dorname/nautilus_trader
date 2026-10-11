//! GUI 无头走查（gui-walkthrough 提案）：egui_kittest 无头驱动真实
//! `ResearchApp::render_root`，把 swarm-scene-entry 落地的两条多 Agent 旅程
//! （S17 确认需求→三路分析→产物挂载；S19 AI 整理参数→填表→二次确认停留）
//! 从 [manual] 目检升级为自动化证据。
//!
//! 背景：issue #1/#32 表明当前 WSL2/WSLg 环境无可见窗口，人工走查不可用；
//! 本套件延续 ST-S15-06 的无头先例，经 AccessKit 树完成导航、点击与断言，
//! 并尽力（best-effort）经 wgpu 软件渲染落盘关键帧截图到
//! `target/gui-walkthrough/`（无可用 GPU 后端时跳过，不影响判定）。
//!
//! ST-GUI-01/02 调用真实 LLM（moonshot-coding，profile 缺省取
//! ~/.octos/profiles/octos.json），会产生 API 调用；与 wire_real_llm 同口径。
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use nautilus_research_desktop::agent_bridge::AgentSessionConfig;
use nautilus_research_desktop::app::ResearchApp;
use nautilus_research_desktop::nav::{ALL_ROUTES, Route};
use nautilus_research_desktop::theme;
use nautilus_research_testkit::case;

/// 无头走查会话：真实 app + 独立临时工作区（协调器桥就地落盘）。
struct Walk {
    harness: Harness<'static, ResearchApp>,
}

/// 是否启用 wgpu 渲染截图（embed-octos-runtime）。
///
/// 串行模式（`RUST_TEST_THREADS=1` 或 `--test-threads=1`，openlogos verify
/// 的固定跑法）自动降级为无渲染：跑过 wgpu 软渲染（llvmpipe/LLVM JIT）的
/// 线程在退出阶段存在 glibc TLS 析构竞态（SIGSEGV，Mesa/LLVM 层，断言与
/// AccessKit 布局均不受影响）；并行模式保留 best-effort 截图并经
/// `walk_finished` 的 exit(0) 守卫收尾。截图是加分证据，断言是判定口径。
fn render_enabled() -> bool {
    if std::env::var_os("RESEARCH_GUI_WALK_RENDER")
        .is_some_and(|v| v == "0")
    {
        return false;
    }
    if std::env::var_os("RUST_TEST_THREADS").is_some_and(|v| v == "1") {
        return false;
    }
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--test-threads") {
        if args.get(i + 1).is_some_and(|v| v == "1") {
            return false;
        }
    }
    if args
        .iter()
        .any(|a| a.starts_with("--test-threads=1"))
    {
        return false;
    }
    true
}

/// 本文件全部测试函数名（计算计划跑数用，与下方 fn 声明保持同步）。
const TEST_NAMES: [&str; 6] = [
    "st_gui_journey_real_llm",
    "ut_gui_01_render_and_nav",
    "ut_gui_02_empty_state_gating",
    "ut_gui_03_idle_no_taskbar",
    "ut_gui_04_narrow_viewport_slim_sidebar",
    "ut_s17_23_project_isolation",
];

/// 计划运行的测试数：无过滤词 = 全量；有过滤词按 libtest 子串匹配计数
/// （支持单跑 `-- st_gui_journey` 与多过滤 `-- ut_gui` 等）。
fn planned_tests() -> usize {
    let filters: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    if filters.is_empty() {
        return TEST_NAMES.len();
    }
    TEST_NAMES
        .iter()
        .filter(|n| filters.iter().any(|f| n.contains(f.as_str())))
        .count()
}

/// 剩余未收尾的走查测试数（按计划跑数惰性初始化）。
static REMAINING_TESTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// 走查收尾（embed-octos-runtime）：规避测试线程退出阶段 glibc
/// `nptl_deallocate_tsd` 调用已释放 TLS 析构函数的竞态（2026-10-11 gdb 定位
/// 为 SIGSEGV，根源在 Mesa/llvmpipe 软渲染层，功能断言与截图均不受影响）。
/// 各测试必须以 `finish(walk)` / `walk_finished()` 收尾；最后一个收尾时以
/// `exit(0)` 结束进程，跳过线程退出清理（reporter 逐 case 即时落盘，
/// libtest 逐条 ok 输出均已打印，不受影响）。
fn walk_finished() {
    use std::sync::atomic::Ordering;
    std::thread::sleep(Duration::from_millis(200));
    // 惰性初始化计划数（0=未初始化；并发窗口下重复写入同值，幂等无害）
    if REMAINING_TESTS.load(Ordering::SeqCst) == 0 {
        REMAINING_TESTS.store(planned_tests(), Ordering::SeqCst);
    }
    if REMAINING_TESTS.fetch_sub(1, Ordering::SeqCst) == 1 {
        // exit 异步化（embed-octos-runtime 修正）：finish 在 case 闭包内执行，
        // 直接 exit 会抢在 case() 的 report_result 之前（最后一个收尾测试的
        // 上报丢失，实测 UT-S17-23）。延迟线程让闭包先返回完成上报，再退出。
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(800));
            std::process::exit(0);
        });
    }
}

fn finish(w: Walk) {
    drop(w);
    walk_finished();
}

fn temp_root(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "rd-gui-walk-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("创建走查临时目录");
    dir
}

fn build_walk(tag: &str, llm: bool) -> Walk {
    let ws = temp_root(tag).join("workspace");
    std::fs::create_dir_all(&ws).expect("创建走查工作区");
    if llm {
        // SAFETY：本套件仅 ST 旅程（单线程串行旅程内）读写该进程级环境变量；
        // UT 用例不触发 agent_data_dir（worker 惰性创建），无并发读写。
        unsafe { std::env::set_var("RESEARCH_WORKSPACE", &ws) };
    }
    let mut app = ResearchApp::new(false);
    app.attach_bridge(ws);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_ui_state(|ui, app: &mut ResearchApp| app.render_root(ui), app);
    // 内嵌 Noto Sans SC：保证中文标签进入 AccessKit 树与截图一致
    theme::setup_fonts(&harness.ctx);
    harness.run_steps(2);
    Walk { harness }
}

/// AccessKit 树中是否存在包含指定子串的标签（聊天气泡 / 卡片 / 任务条均为 label）。
fn has_text(h: &Harness<'_, ResearchApp>, needle: &str) -> bool {
    h.query_all_by_label_contains(needle).next().is_some()
}

/// 经侧栏导航到指定路由（nav_item 已带 widget_info，角色为 Button）。
fn goto(w: &mut Walk, route: Route) {
    w.harness
        .get_by_role_and_label(egui::accesskit::Role::Button, route.title())
        .click_accesskit();
    w.harness.run_steps(3);
    assert_eq!(
        w.harness.state().test_current_route(),
        route,
        "侧栏导航应切换到 {}",
        route.title()
    );
}

/// 点击按钮标签（精确匹配、Button 角色）：经 AccessKit Action 触发，
/// 不受滚动视口裁剪影响（折叠区头往往在首屏之外，指针点击会被裁掉）。
fn click_button(w: &mut Walk, label: &str) {
    w.harness
        .get_by_role_and_label(egui::accesskit::Role::Button, label)
        .click_accesskit();
    w.harness.run_steps(3);
}

/// 展开默认折叠的协调器运行参数区（「AI 整理参数」位于其中）。
fn open_run_params_section(w: &mut Walk) {
    click_button(w, "运行参数（协调器）");
}

/// 泵帧直到谓词满足或超时（后台 worker 以真实时间推进，帧间 sleep）。
fn pump_until(
    w: &mut Walk,
    timeout: Duration,
    mut pred: impl FnMut(&Harness<'_, ResearchApp>) -> bool,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        w.harness.step();
        if pred(&w.harness) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// 尽力截图：wgpu 软件渲染成功则落盘 PNG，失败仅记录（不阻断走查判定）。
fn snap(w: &mut Walk, name: &str) {
    if !render_enabled() {
        eprintln!("[gui-walkthrough] 串行/无渲染模式，跳过截图 {name}（布局断言仍有效）");
        return;
    }
    match w.harness.render() {
        Ok(img) => {
            let dir =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/gui-walkthrough");
            if std::fs::create_dir_all(&dir).is_err() {
                return;
            }
            let path = dir.join(format!("{name}.png"));
            match img.save(&path) {
                Ok(()) => eprintln!("[gui-walkthrough] 截图已落盘：{}", path.display()),
                Err(e) => eprintln!("[gui-walkthrough] 截图保存失败（忽略）：{e}"),
            }
        }
        Err(e) => eprintln!("[gui-walkthrough] 无可用渲染后端，跳过截图 {name}：{e}"),
    }
}

/// UT-GUI-01：无头渲染关键路由不 panic；「确认需求」按钮可见可定位；
/// 侧栏 11 路由全部以 Button 角色可达。
#[test]
fn ut_gui_01_render_and_nav() {
    case("UT-GUI-01", || {
        let mut w = build_walk("ut01", false);
        // 11 条路由逐一导航：每页渲染不 panic，且侧栏项始终可达
        for route in ALL_ROUTES {
            goto(&mut w, route);
        }
        // 需求页确认按钮可见可定位（不点击：点击会真实派发 LLM 任务）
        goto(&mut w, Route::Requirements);
        w.harness
            .get_by_role_and_label(egui::accesskit::Role::Button, "确认需求");
        snap(&mut w, "ut-gui-01-requirements");
        finish(w);
    });
}

/// UT-GUI-02：空态（无实验）结构断言——空态引导「运行合成实验」可见；
/// 「运行参数（协调器）」折叠区（含「AI 整理参数」）按 UI 口径不渲染。
#[test]
fn ut_gui_02_empty_state_gating() {
    case("UT-GUI-02", || {
        let mut w = build_walk("ut02", false);
        goto(&mut w, Route::Experiments);
        assert!(
            w.harness
                .query_all_by_role_and_label(
                    egui::accesskit::Role::Button,
                    "运行合成实验"
                )
                .next()
                .is_some(),
            "空态应出现「运行合成实验」引导按钮"
        );
        assert!(
            w.harness
                .query_all_by_label_contains("运行参数（协调器）")
                .next()
                .is_none(),
            "无实验时运行参数折叠区不应渲染（UI 既有口径）"
        );
        finish(w);
    });
}

/// UT-GUI-03：无任务时任务条不出现（无「多 Agent 分析」/「取消」节点）。
#[test]
fn ut_gui_03_idle_no_taskbar() {
    case("UT-GUI-03", || {
        let mut w = build_walk("ut03", false);
        w.harness.run_steps(2);
        assert!(
            !has_text(&w.harness, "多 Agent 分析"),
            "空闲态不应出现多 Agent 任务行"
        );
        assert!(
            w.harness
                .query_all_by_role_and_label(egui::accesskit::Role::Button, "取消")
                .next()
                .is_none(),
            "空闲态不应出现任务条取消按钮"
        );
        finish(w);
    });
}

/// UT-GUI-04：窄视口（880px）布局对照（desktop-window-bootstrap 提案，
/// issue #7）——侧栏按 Slim 计划收成 65px 图标栏；截图落盘
/// docs/research-desktop/screenshots/ 作原型 ≤900px 对照证据。
#[test]
fn ut_gui_04_narrow_viewport_slim_sidebar() {
    case("UT-GUI-04", || {
        let ws = temp_root("ut04").join("workspace");
        std::fs::create_dir_all(&ws).expect("创建走查工作区");
        let mut app = ResearchApp::new(false);
        app.attach_bridge(ws);
        // 880px ≤ 900 断点：应触发 Slim 图标栏
        let mut harness = Harness::builder()
            .with_size(egui::vec2(880.0, 720.0))
            .build_ui_state(|ui, app: &mut ResearchApp| app.render_root(ui), app);
        theme::setup_fonts(&harness.ctx);
        harness.run_steps(3);

        // Slim 口径断言：导航项宽 ≈ 65px 图标栏内容宽（远小于宽档 178/212）
        let nav = harness.get_by_role_and_label(egui::accesskit::Role::Button, "需求文档");
        let w = nav.rect().width();
        assert!(
            w < 100.0,
            "880px 窄视口导航项应收成图标栏宽度（<100px），实测 {w}"
        );

        // 对照宽档：1440px 时导航项为完整宽度
        let ws2 = temp_root("ut04w").join("workspace");
        std::fs::create_dir_all(&ws2).expect("创建走查工作区");
        let mut app2 = ResearchApp::new(false);
        app2.attach_bridge(ws2);
        let mut wide = Harness::builder()
            .with_size(egui::vec2(1440.0, 900.0))
            .build_ui_state(|ui, app: &mut ResearchApp| app.render_root(ui), app2);
        theme::setup_fonts(&wide.ctx);
        wide.run_steps(3);
        let nav_wide =
            wide.get_by_role_and_label(egui::accesskit::Role::Button, "需求文档");
        assert!(
            nav_wide.rect().width() > 150.0,
            "1440px 宽档导航项应为完整宽度（>150px），实测 {}",
            nav_wide.rect().width()
        );

        // 截图证据落盘 docs（原型 ≤900px 对照；串行/无渲染模式跳过）
        if !render_enabled() {
            eprintln!("[gui-walkthrough] 串行/无渲染模式，跳过 UT-GUI-04 截图（布局断言仍有效）");
        } else {
        match harness.render() {
            Ok(img) => {
                let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../docs/research-desktop/screenshots");
                if std::fs::create_dir_all(&dir).is_ok() {
                    let path = dir.join("ut-gui-04-narrow.png");
                    match img.save(&path) {
                        Ok(()) => eprintln!("[gui-walkthrough] 窄视口截图：{}", path.display()),
                        Err(e) => eprintln!("[gui-walkthrough] 窄视口截图保存失败（忽略）：{e}"),
                    }
                }
            }
            Err(e) => {
                eprintln!("[gui-walkthrough] 无渲染后端，UT-GUI-04 截图跳过（布局断言仍有效）：{e}")
            }
        }
        }
        // embed-octos-runtime：本用例直接持有两个 Harness，显式释放后统一收尾。
        drop(harness);
        drop(wide);
        walk_finished();
    });
}

/// UT-S17-23：OUP 会话项目隔离（oup-session-config 提案，issue #27）——
/// 两项目 session_id/数据目录互不相同；安装 worker 后切换项目，worker 重置、
/// 再次惰性创建时绑定新项目 session。
#[test]
fn ut_s17_23_project_isolation() {
    case("UT-S17-23", || {
        // 配置层：两项目 session_id 与数据目录互不相同
        let d1 = temp_root("iso1");
        let d2 = temp_root("iso2");
        let c1 = AgentSessionConfig::desktop(&d1, "1");
        let c2 = AgentSessionConfig::desktop(&d2, "2");
        assert_ne!(c1.session_id, c2.session_id, "不同项目 session_id 必须不同");
        assert!(c1.session_id.ends_with("-1") && c2.session_id.ends_with("-2"));
        assert_ne!(c1.oup.data_dir, c2.oup.data_dir, "数据目录必须按项目隔离");

        // GUI 层：worker 随项目切换重置并重绑新 session
        let mut w = build_walk("ut23", true);
        w.harness.state_mut().test_spawn_workers();
        let (a1, s1) = w.harness.state().test_worker_sessions();
        assert!(a1.is_some() && s1.is_some(), "worker 应已惰性创建");
        assert!(
            a1.as_deref().is_some_and(|s| s.ends_with("-1")),
            "初始 worker 应绑项目 1：{a1:?}"
        );

        let new_id = w
            .harness
            .state_mut()
            .test_add_project_and_switch("第二项目");
        let (a2, s2) = w.harness.state().test_worker_sessions();
        assert!(
            a2.is_none() && s2.is_none(),
            "切换项目后旧 worker 必须被重置（不串旧 session）"
        );

        w.harness.state_mut().test_spawn_workers();
        let (a3, _) = w.harness.state().test_worker_sessions();
        assert!(
            a3.as_deref()
                .is_some_and(|s| s.ends_with(&format!("-{new_id}"))),
            "重建 worker 应绑新项目 session：{a3:?}"
        );
        finish(w);
    });
}

/// ST-GUI-01/02（真实 LLM，单会话顺序旅程）：
/// 确认需求 → 任务条三路进度行 → 完成卡 → swarm_analysis 挂载；
/// 保存设计/策略版本 → 合成实验 → AI 整理参数 → 表单填入 → 停留等待二次确认。
#[test]
fn st_gui_journey_real_llm() {
    let mut w = build_walk("st", true);

    // ---- ST-GUI-01：S17 确认需求派发三路多 Agent 分析（issue #26）
    case("ST-GUI-01", std::panic::AssertUnwindSafe(|| {
        goto(&mut w, Route::Requirements);
        // 非交互固定输入与 ST-S17-19 一致，保留真实派发与产物挂载断言。
        w.harness.state_mut().ai_req_text = "研究日线双均线策略在震荡市的表现：已有 OHLCV；使用三个固定合成标的；收盘形成信号、下一交易日开盘执行；投入上限100%，佣金万三，现金不足拒绝订单。这是非交互固定样本分析，所有前提已给定。各路直接用一到两句话返回分析；不要向用户提问，也不要调用任何工具。未知量列为限制即可。".into();
        w.harness.run_steps(2);
        click_button(&mut w, "确认需求");

        // 派发后任务条出现三路进度行（多 Agent 分析 R1 · n/3 · Ns）
        let dispatched = pump_until(&mut w, Duration::from_secs(15), |h| {
            has_text(h, "多 Agent 分析 R1") && has_text(h, "/3")
        });
        assert!(dispatched, "确认需求后任务条应出现三路进度行");
        snap(&mut w, "st-gui-01-taskbar");

        // 终态：完成卡出现（worker 超时 600s，泵帧预算留足余量）
        let completed = pump_until(&mut w, Duration::from_secs(700), |h| {
            has_text(h, "多 Agent 分析完成") || has_text(h, "多 Agent 分析未全部成功")
        });
        assert!(completed, "三路分析应在超时前到达终态");
        assert!(
            has_text(&w.harness, "多 Agent 分析完成"),
            "三路分析应全部成功并产出完成卡"
        );

        // 产物摘要已挂到需求版本 R1
        let summary = w.harness.state().test_requirement_swarm_analysis(1);
        assert!(
            summary.is_some_and(|s| !s.trim().is_empty()),
            "R1 需求版本应挂载多 Agent 分析摘要"
        );
        snap(&mut w, "st-gui-01-completed");
    }));

    // ---- ST-GUI-02：S19 LLM 整理参数填表，提交仍需人工二次确认（issue #15）
    case("ST-GUI-02", std::panic::AssertUnwindSafe(|| {
        // 设计版本是整理参数的前置（本地保存，不经 LLM）
        goto(&mut w, Route::Design);
        click_button(&mut w, "保存设计版本");
        w.harness.step();

        // 策略版本（本地检查 + 保存），解锁合成实验入口
        goto(&mut w, Route::Develop);
        click_button(&mut w, "检查草稿");
        w.harness.step();
        click_button(&mut w, "保存版本");
        w.harness.step();

        // UI 口径：运行参数折叠区仅在已有实验时渲染，先跑一次本地合成实验
        goto(&mut w, Route::Experiments);
        click_button(&mut w, "运行合成实验");
        let ran = pump_until(&mut w, Duration::from_secs(30), |h| {
            has_text(h, "实验 E1 完成")
        });
        assert!(ran, "合成实验应即时完成并出现提示");

        let form_before = w.harness.state().test_run_form().clone();
        open_run_params_section(&mut w);
        click_button(&mut w, "AI 整理参数");

        // 任务条出现 LLM 任务行
        let started = pump_until(&mut w, Duration::from_secs(15), |h| {
            has_text(h, "整理实验参数 ·")
        });
        assert!(started, "点击整理参数后应出现 LLM 任务行");

        // 终态：参数填入卡出现（worker turn 超时 420s，泵帧预算留余量）
        let filled = pump_until(&mut w, Duration::from_secs(500), |h| {
            has_text(h, "运行参数已填入") || has_text(h, "参数整理失败")
        });
        assert!(filled, "整理参数应在超时前到达终态");
        assert!(
            has_text(&w.harness, "运行参数已填入"),
            "模型整理参数应成功填入运行表单"
        );

        // 表单被真实填入：起止日期非空（缺省为空串）、fast<slow、top_k>0
        let form = w.harness.state().test_run_form();
        assert!(!form.start.is_empty(), "整理后起始日期应非空");
        assert!(!form.end.is_empty(), "整理后结束日期应非空");
        assert!(form.fast < form.slow, "整理后 fast 应小于 slow");
        assert!(form.top_k > 0, "整理后 top_k 应大于 0");
        assert_ne!(
            (form.start.clone(), form.end.clone()),
            (form_before.start, form_before.end),
            "整理后区间应较缺省表单发生变化"
        );

        // 二次确认语义：填表后导航到回测实验页等待人工核对，未自动提交
        assert_eq!(
            w.harness.state().test_current_route(),
            Route::Experiments,
            "填表后应导航到回测实验页等待核对"
        );
        assert!(
            !has_text(&w.harness, "整理实验参数 ·"),
            "任务终态后任务条 LLM 行应消失"
        );
        snap(&mut w, "st-gui-02-form-filled");
    }));
    finish(w);
}
