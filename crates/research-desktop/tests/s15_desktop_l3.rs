//! S15 桌面批次 L3（流水线五页对接）场景测试：页面状态机 + 协调器桥全链路。
//!
//! 规格对齐：logos/resources/test/core-S15-test-cases.md（UT-S15-09/10、ST-S15-04/05/06）。
//! 测试不经图形后端：以 DesktopBridge + TaskWatch 状态机驱动真实协调器
//! （进程内执行器，temp workspace），与 GUI 渲染路径共享同一套模型函数；
//! ST-S15-06 另以 egui Context::run_ui 无头驱动真实 app 渲染根 Ui 做整页几何回归。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nautilus_research_desktop::bridge::{
    CompareForm, DesktopBridge, ImportForm, PlanForm, RunForm, UniverseForm,
};
use nautilus_research_desktop::pipeline::{TaskWatch, is_success, terminal_error_text};
use nautilus_research_testkit::case;

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "research-s15-l3-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn write_file(dir: &Path, name: &str, content: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("写入暂存文件");
    path.to_string_lossy().to_string()
}

/// 24 个交易日行情（2023-12-04 起，收盘 10..33 阶梯上行）。
fn rising_bars(code: &str, dir: &Path, name: &str) -> String {
    let dates = [
        "2023-12-04",
        "2023-12-05",
        "2023-12-06",
        "2023-12-07",
        "2023-12-08",
        "2023-12-11",
        "2023-12-12",
        "2023-12-13",
        "2023-12-14",
        "2023-12-15",
        "2023-12-18",
        "2023-12-19",
        "2023-12-20",
        "2023-12-21",
        "2023-12-22",
        "2023-12-25",
        "2023-12-26",
        "2023-12-27",
        "2023-12-28",
        "2023-12-29",
        "2024-01-02",
        "2024-01-03",
        "2024-01-04",
        "2024-01-05",
    ];
    let mut body =
        String::from("instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n");
    for (i, d) in dates.iter().enumerate() {
        let close = 10 + i as i32;
        body.push_str(&format!(
            "{code},{d},{close},{close},{close},{close},100000,{close}0000\n"
        ));
    }
    write_file(dir, name, &body)
}

const RULE_ALL_PASS: &str = r#"{"op":"and","children":[{"field":"close","op":"gte","value":"0"}]}"#;

/// GUI 形态的等待：以页面轮询语义（get_task + TaskWatch::poll）推进到终态。
fn watch_to_terminal(bridge: &DesktopBridge, watch: &mut TaskWatch) -> bool {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let Some(task_id) = watch.task_id().map(str::to_string) else {
            return false;
        };
        let Ok(view) = bridge.get_task(&task_id) else {
            return false;
        };
        watch.poll(view);
        if !watch.is_active() {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// UT-S15-09：页面状态机（提交→轮询→终态落定；错误语义；终态后不推进）。
#[test]
fn ut_s15_09_page_state_machine() {
    case("UT-S15-09", || {
        let mut w = TaskWatch::Idle;
        assert!(!w.is_active());
        assert!(w.task_id().is_none());

        w = TaskWatch::submitted("T1", "导入中…");
        assert!(w.is_active());
        assert_eq!(w.task_id(), Some("T1"));

        let view = |task_id: &str, succeeded: bool| nautilus_research_domain::protocol::TaskView {
            task_id: task_id.into(),
            state: if succeeded {
                nautilus_research_domain::TaskState::Succeeded
            } else {
                nautilus_research_domain::TaskState::Running
            },
            last_seq: 1,
            progress: None,
            artifact_hash: None,
            snapshot_id: None,
            error: None,
            children: Vec::new(),
            parent_id: None,
        };

        // 运行中推进；其他任务视图不推进
        assert!(w.poll(view("T1", false)));
        assert!(w.is_active());
        assert!(!w.poll(view("OTHER", true)));
        assert!(w.is_active());

        // 终态落定：停止活跃；终态后不推进
        assert!(w.poll(view("T1", true)));
        assert!(!w.is_active());
        assert!(w.terminal().map(is_success).unwrap_or(false));
        assert!(!w.poll(view("T1", false)));
        assert!(w.terminal().map(is_success).unwrap_or(false));

        // 拒绝错误行：错误码契约原文 + 中文消息
        let e = nautilus_research_domain::ResearchError::not_found("快照不存在：SNAP-404");
        assert_eq!(
            nautilus_research_desktop::pipeline::rejection_text(&e),
            "（NOT_FOUND）：快照不存在：SNAP-404"
        );
        // 终态错误行提取
        let mut failed = view("T2", true);
        failed.state = nautilus_research_domain::TaskState::Failed;
        failed.error = Some(nautilus_research_domain::protocol::ErrorBody {
            code: "IDEMPOTENCY_CONFLICT".into(),
            message: "相同幂等键对应不同请求".into(),
            field: None,
            retryable: false,
        });
        assert_eq!(
            terminal_error_text(&failed).as_deref(),
            Some("任务失败（IDEMPOTENCY_CONFLICT）：相同幂等键对应不同请求")
        );

        // PageState：提交成功清错误；提交失败复位 Idle 并保留错误
        let mut ps: nautilus_research_desktop::pipeline::PageState<TaskWatch> = Default::default();
        ps.on_submitted("T3", "预览中…");
        assert!(ps.error.is_none() && ps.watch.is_active());
        ps.on_submit_failed("缺少必填参数");
        assert!(!ps.watch.is_active());
        assert_eq!(
            ps.error.as_ref().map(|e| e.text.as_str()),
            Some("缺少必填参数")
        );
    });
}

/// ST-S15-04：桌面桥全链路——导入→预览→保存→两次运行→比较→计划→导出→备注
/// （GUI 长驻协调器形态；轮询语义与页面状态机一致）。
#[test]
fn st_s15_04_desktop_bridge_full_pipeline() {
    case("ST-S15-04", || {
        let ws = temp_workspace("st04");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let bridge = DesktopBridge::open(ws.clone()).expect("打开工作区");

        // —— 数据快照页：导入 ——
        let mut watch = {
            let form = ImportForm {
                source: 0,
                paths: vec![quotes.clone()],
                price_basis: 0,
                auxiliary_kind: None,
            };
            let r = bridge.submit_import(&form).expect("导入提交");
            TaskWatch::submitted(r.task_id, "导入中…")
        };
        assert!(watch_to_terminal(&bridge, &mut watch), "导入应到终态");
        let import_view = watch.terminal().expect("终态视图").clone();
        assert!(is_success(&import_view), "导入成功：{import_view:?}");
        let snapshot_id = import_view.snapshot_id.clone().expect("快照 ID");

        // —— 股票池页：预览 + 保存 ——
        let mut watch = {
            let form = UniverseForm {
                rule_text: RULE_ALL_PASS.into(),
                as_of: "2024-01-05".into(),
                strict: true,
                fixed_membership: true,
                ignore_missing: false,
            };
            let r = bridge
                .submit_preview(&snapshot_id, &form)
                .expect("预览提交");
            TaskWatch::submitted(r.task_id, "预览中…")
        };
        assert!(watch_to_terminal(&bridge, &mut watch), "预览应到终态");
        assert!(is_success(watch.terminal().expect("终态视图")));
        let preview_task = watch.task_id().expect("预览任务 ID").to_string();
        let p = bridge.preview(&preview_task).expect("预览结果");
        let universe = bridge
            .save_universe(&preview_task, &p.preview_hash, &p.input_hash, "ST-S15-04")
            .expect("保存股票池");

        // —— 运行页：两次运行（零费率 EMA1/2 与非零佣金 EMA1/2）——
        let run_form = |rate: &str| RunForm {
            fast: 1,
            slow: 2,
            top_k: 1,
            start: "2023-12-04".into(),
            end: "2024-01-05".into(),
            capital: "20000".into(),
            commission_rate: rate.into(),
            min_commission: "0".into(),
            sell_tax: "0".into(),
            other_fee: "0".into(),
            slippage_bps: "0".into(),
            participation: "1".into(),
            eff_start: "2023-12-01".into(),
            eff_end: "2024-02-01".into(),
        };
        let submit_run = |rate: &str| {
            let r = bridge
                .submit_run(&snapshot_id, &universe.universe_id, &run_form(rate))
                .expect("运行提交");
            let mut w = TaskWatch::submitted(r.task_id, "回测运行中…");
            assert!(watch_to_terminal(&bridge, &mut w), "运行应到终态");
            let v = w.terminal().expect("终态视图").clone();
            assert!(is_success(&v), "运行成功：{v:?}");
            v.task_id
        };
        let run_a = submit_run("0");
        let run_b = submit_run("0.001");

        // —— 比较页：双方指标 + 费用差异 ——
        let cmp = bridge
            .compare(&CompareForm::default(), vec![run_a.clone(), run_b.clone()])
            .expect("比较");
        assert_eq!(cmp.runs.len(), 2, "双方指标行");
        assert!(
            cmp.differences.iter().any(|d| d.kind == "cost"),
            "费用差异显式列出：{cmp:?}"
        );

        // —— 计划页：生成 → 导出 → 备注 ——
        let holdings = serde_json::json!({
            "as_of": "2024-01-05",
            "cash_cny": "10000",
            "positions": [
                {"instrument_id": "SYN-A", "quantity": 100, "sellable_quantity": 100}
            ],
            "total_assets_cny": "20000"
        })
        .to_string();
        let plan_form = PlanForm {
            as_of: "2024-01-05".into(),
            holdings_json: holdings,
            export_path: ws.join("导出 目录/计划.csv").to_string_lossy().to_string(),
            note_text: "已人工核对".into(),
        };
        let mut watch = {
            let r = bridge
                .submit_plan(
                    &snapshot_id,
                    &universe.universe_id,
                    &plan_form,
                    &run_form("0"),
                )
                .expect("计划提交");
            TaskWatch::submitted(r.task_id, "计划生成中…")
        };
        assert!(watch_to_terminal(&bridge, &mut watch), "计划应到终态");
        assert!(is_success(watch.terminal().expect("终态视图")));
        let plan_id = watch.task_id().expect("计划任务 ID").to_string();

        let receipt = bridge
            .export_plan(&plan_id, &plan_form.export_path)
            .expect("导出");
        let bytes = std::fs::read(&plan_form.export_path).expect("读取导出文件");
        assert_eq!(
            nautilus_research_domain::hash::sha256_hex(&bytes),
            receipt.sha256,
            "导出哈希与回执一致"
        );
        bridge
            .save_note(&plan_id, "已人工核对", "备注")
            .expect("保存备注");

        // —— 运行页取消：长驻协调器下取消直达（完成则 ALREADY_TERMINAL 容双态）——
        let r = bridge
            .submit_run(&snapshot_id, &universe.universe_id, &run_form("0"))
            .expect("运行提交");
        let task_id = r.task_id.clone();
        let mut watch = TaskWatch::submitted(r.task_id, "回测运行中…");
        let cancel = bridge.cancel(&task_id);
        match cancel {
            Ok(v) => assert_eq!(v.state, nautilus_research_domain::TaskState::Cancelling),
            Err(e) => {
                // 进程内执行器很快，任务可能已成功：ALREADY_TERMINAL 合法
                assert_eq!(
                    serde_json::to_value(&e.code).unwrap(),
                    serde_json::json!("ALREADY_TERMINAL"),
                    "取消拒绝仅允许已终态：{e:?}"
                );
            }
        }
        assert!(watch_to_terminal(&bridge, &mut watch), "取消后应到终态");
        let v = watch.terminal().expect("终态视图");
        assert!(
            is_success(v) || v.state == nautilus_research_domain::TaskState::Cancelled,
            "终态与取消竞争一致：{v:?}"
        );
    });
}

/// UT-S15-10：净值曲线接线纯函数——EquityDoc 解析、最近两次选取、三态归因横幅（RD-005）。
#[test]
fn ut_s15_10_equity_points_and_banner() {
    case("UT-S15-10", || {
        use nautilus_research_desktop::bridge::equity_points;
        use nautilus_research_desktop::workspace::Workspace;
        use nautilus_research_domain::worker_api::EquityDoc;

        let doc = |d: &str, e: &str| EquityDoc {
            trade_date: d.into(),
            cash_cny: "0".into(),
            positions_value_cny: "0".into(),
            receivables_cny: "0".into(),
            equity_cny: e.into(),
        };
        // 解析保序（日期原样、十进制净值转 f64）
        let pts = equity_points(vec![
            doc("2024-01-02", "10000.50"),
            doc("2024-01-03", "10010"),
            doc("2024-01-04", "9999.9"),
        ])
        .expect("合法净值解析");
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].date, "2024-01-02");
        assert!((pts[0].value - 10000.50).abs() < 1e-9);
        assert_eq!(pts[2].date, "2024-01-04");
        assert!((pts[2].value - 9999.9).abs() < 1e-9);
        // 空序列得空
        assert!(equity_points(vec![]).expect("空序列").is_empty());
        // 非法净值显式报错，不静默丢点
        let err =
            equity_points(vec![doc("2024-01-02", "一万"), doc("2024-01-03", "10010")]).unwrap_err();
        assert_eq!(
            serde_json::to_value(&err.code).unwrap(),
            serde_json::json!("INVALID_ARGUMENT"),
            "非法净值报 INVALID_ARGUMENT：{err:?}"
        );

        // 最近两次有任务实验选取（新→旧；无任务证据的实验跳过）
        let mut w = Workspace::new();
        w.confirm_requirement("量价选股", "夏普>0", 30.0, 1_000_000.0)
            .unwrap();
        w.generate_design_from_req("EMA 双均线").unwrap();
        w.save_version("fn v1() {}").unwrap();
        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        let e2 = w.run_experiment(None).unwrap(); // 无任务证据
        let e3 = w.run_experiment(Some("T3".into())).unwrap();
        let latest = w.latest_task_experiments(2);
        assert_eq!(latest.len(), 2, "无任务实验被跳过");
        assert_eq!(latest[0], (e3, 1, "T3".to_string()));
        assert_eq!(latest[1], (e1, 1, "T1".to_string()));
        assert_eq!(w.latest_task_experiments(1)[0].0, e3);
        let _ = e2;

        // 三态归因横幅（原型 comparisonStatus 语义）
        // 同版本 → 重复实验
        let b = w.comparison_banner(e1, e3).expect("同版本横幅");
        assert!(b.contains("同输入、同源码的重复实验"), "{b}");
        // 输入一致、代码不同 → 可归因（同 R/D 下保存 v2）
        w.save_version("fn v2() {}").unwrap();
        let e4 = w.run_experiment(Some("T4".into())).unwrap();
        let b = w.comparison_banner(e3, e4).expect("可归因横幅");
        assert!(
            b.contains("冻结输入一致，代码差异可作为受控比较因素"),
            "{b}"
        );
        assert!(!b.contains("仅并列查看"), "{b}");
        // 输入不同 → 仅并列查看（新需求 → 新设计 → v3）
        w.confirm_requirement("量价 v2", "夏普>0.5", 40.0, 100.0)
            .unwrap();
        w.generate_design_from_req("EMA v2").unwrap();
        w.save_version("fn v3() {}").unwrap();
        let e5 = w.run_experiment(Some("T5".into())).unwrap();
        let b = w.comparison_banner(e4, e5).expect("输入不同横幅");
        assert!(b.contains("仅并列查看，不作代码效果归因"), "{b}");
        assert!(b.contains("需求版本"), "{b}");
        // 实验不存在
        assert_eq!(w.comparison_banner(e1, 999).unwrap_err(), "实验不存在");
    });
}

/// ST-S15-05：桌面桥净值曲线读取——合成数据两次运行后经 query_rows equity 桶拉取（RD-005）。
#[test]
fn st_s15_05_equity_curve_read() {
    case("ST-S15-05", || {
        let ws = temp_workspace("st05");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let bridge = DesktopBridge::open(ws.clone()).expect("打开工作区");

        // —— 导入 → 预览 → 保存（同 ST-S15-04 链路）——
        let mut watch = {
            let form = ImportForm {
                source: 0,
                paths: vec![quotes.clone()],
                price_basis: 0,
                auxiliary_kind: None,
            };
            let r = bridge.submit_import(&form).expect("导入提交");
            TaskWatch::submitted(r.task_id, "导入中…")
        };
        assert!(watch_to_terminal(&bridge, &mut watch), "导入应到终态");
        let import_view = watch.terminal().expect("终态视图").clone();
        assert!(is_success(&import_view), "导入成功：{import_view:?}");
        let snapshot_id = import_view.snapshot_id.clone().expect("快照 ID");

        let mut watch = {
            let form = UniverseForm {
                rule_text: RULE_ALL_PASS.into(),
                as_of: "2024-01-05".into(),
                strict: true,
                fixed_membership: true,
                ignore_missing: false,
            };
            let r = bridge
                .submit_preview(&snapshot_id, &form)
                .expect("预览提交");
            TaskWatch::submitted(r.task_id, "预览中…")
        };
        assert!(watch_to_terminal(&bridge, &mut watch), "预览应到终态");
        assert!(is_success(watch.terminal().expect("终态视图")));
        let preview_task = watch.task_id().expect("预览任务 ID").to_string();
        let p = bridge.preview(&preview_task).expect("预览结果");
        let universe = bridge
            .save_universe(&preview_task, &p.preview_hash, &p.input_hash, "ST-S15-05")
            .expect("保存股票池");

        // —— 两次运行 ——
        let run_form = |rate: &str| RunForm {
            fast: 1,
            slow: 2,
            top_k: 1,
            start: "2023-12-04".into(),
            end: "2024-01-05".into(),
            capital: "20000".into(),
            commission_rate: rate.into(),
            min_commission: "0".into(),
            sell_tax: "0".into(),
            other_fee: "0".into(),
            slippage_bps: "0".into(),
            participation: "1".into(),
            eff_start: "2023-12-01".into(),
            eff_end: "2024-02-01".into(),
        };
        let submit_run = |rate: &str| {
            let r = bridge
                .submit_run(&snapshot_id, &universe.universe_id, &run_form(rate))
                .expect("运行提交");
            let mut w = TaskWatch::submitted(r.task_id, "回测运行中…");
            assert!(watch_to_terminal(&bridge, &mut w), "运行应到终态");
            let v = w.terminal().expect("终态视图").clone();
            assert!(is_success(&v), "运行成功：{v:?}");
            v.task_id
        };
        let run_a = submit_run("0");
        let run_b = submit_run("0.001");

        // —— 净值曲线：非空、日期升序、净值有限且为正 ——
        for run in [&run_a, &run_b] {
            let curve = bridge.equity_curve(run).expect("净值曲线读取");
            assert!(!curve.is_empty(), "{run} 净值曲线非空");
            for w in curve.windows(2) {
                assert!(
                    w[0].date < w[1].date,
                    "日期升序：{} 应在 {} 之前",
                    w[0].date,
                    w[1].date
                );
            }
            assert!(
                curve.iter().all(|p| p.value.is_finite() && p.value > 0.0),
                "净值有限且为正：{run}"
            );
        }

        // —— 未完成运行：submit 返回后立即读取（任务行/运行行已于入队时同步登记，
        // 守望线程执行需毫秒级；读取为同线程直读 → 状态必为 Queued/Running）——
        // 注意：spec 须唯一（费率 0.002 未用过），否则幂等回放已完成任务。
        let r = bridge
            .submit_run(&snapshot_id, &universe.universe_id, &run_form("0.002"))
            .expect("运行提交");
        let tid = r.task_id.clone();
        let mut not_ready = false;
        for _ in 0..10 {
            match bridge.equity_curve(&tid) {
                Ok(_) => break, // 执行器竞态完成（曲线可读，非本断言分支）
                Err(e) => {
                    assert_eq!(
                        serde_json::to_value(&e.code).unwrap(),
                        serde_json::json!("RUN_NOT_READY"),
                        "未完成运行读取报 RUN_NOT_READY：{e:?}"
                    );
                    not_ready = true;
                    break;
                }
            }
        }
        assert!(not_ready, "执行完成前读取应报 RUN_NOT_READY，不冒充空曲线");
        // 等终态保持工作区干净（曲线随后可读，主断言已在上方覆盖）
        let mut w = TaskWatch::submitted(tid, "回测运行中…");
        assert!(watch_to_terminal(&bridge, &mut w), "运行应到终态");
    });
}

// ---------------------------------------------------------------- ST-S15-06
/// ST-S15-06：无头整页几何回归——RD-008「子 Ui 继承 horizontal 父布局」缺陷族。
/// egui Context::run_ui 驱动真实 app 渲染根 Ui（不经图形后端、不加载系统字体），
/// 1750×900 与 1100×720 两档各连续三帧，检查 FullOutput.shapes 中 Text 图元的
/// 坐标与 galley 尺寸（RD-008 修复前：workspace-head 垂直居中、页面标题一字一行、
/// 对话内容溢出窗口右缘——本用例五项断言逐一对锁）。
#[test]
fn st_s15_06_headless_page_geometry() {
    case("ST-S15-06", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::layout;

        for (win_w, win_h) in [(1750.0_f32, 900.0_f32), (1100.0, 720.0)] {
            let plan = layout::plan_for_width(win_w);
            let canvas_right = win_w - layout::APP_GAP - layout::chat_w(plan);
            let body_top = layout::APP_GAP + layout::TOPBAR_H;

            let ctx = egui::Context::default();
            let mut app = ResearchApp::new(false);
            let mut shapes = Vec::new();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(win_w, win_h),
                    )),
                    ..Default::default()
                };
                let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
                shapes = std::mem::take(&mut full.shapes);
                // 无头环境不应用纹理增量（字体图集），每帧显式清理避免 drop 断言
                full.drop_without_applying_deltas();
            }
            // (文本, 绘制包围盒)——注意 TextShape.pos 依 galley halign 而定
            // （right_to_left 布局的标签 pos 是文本右上角），统一用 galley.rect 平移求跨度
            let texts: Vec<(String, egui::Rect)> = shapes
                .iter()
                .filter_map(|cs| match &cs.shape {
                    egui::Shape::Text(ts) => Some((
                        ts.galley.text().to_string(),
                        ts.galley.rect.translate(ts.pos.to_vec2()),
                    )),
                    _ => None,
                })
                .collect();
            assert!(!texts.is_empty(), "{win_w}×{win_h}：应绘制出文本图元");
            let find = |needle: &str| {
                texts
                    .iter()
                    .find(|(t, _)| t.contains(needle))
                    .unwrap_or_else(|| panic!("{win_w}×{win_h}：未找到文本「{needle}」"))
            };

            // 1) workspace-head 在 body 顶带（修复前被垂直居中到 body 中央）
            let (_, head_rect) = find("项目产物");
            assert!(
                head_rect.top() < body_top + 44.0 + 10.0,
                "{win_w}×{win_h}：workspace-head 应在 body 顶带（y={}，阈值 {}）",
                head_rect.top(),
                body_top + 54.0
            );

            // 2) 页面标题在画布区内且单行展开（修复前在画布右缘外一字一行）
            let (_, title_rect) = find("让每一步研究，都有依据");
            assert!(
                title_rect.max.x + 5.0 < canvas_right,
                "{win_w}×{win_h}：标题应在画布区内（右端 {}，画布右缘 {canvas_right}）",
                title_rect.max.x
            );
            assert!(
                title_rect.width() >= 100.0,
                "{win_w}×{win_h}：标题应单行展开（宽 {}，一字一行时仅约一字宽）",
                title_rect.width()
            );

            // 3) 对话栏标题在右栏区内（所有实例：header 与消息气泡署名均在对话栏）
            for (t, rect) in texts.iter().filter(|(t, _)| t.contains("研究助手")) {
                assert!(
                    rect.min.x > canvas_right,
                    "{win_w}×{win_h}：「{t}」应在对话栏区（x={}，画布右缘 {canvas_right}）",
                    rect.min.x
                );
            }

            // 3b) 欢迎产物卡标题落在对话栏（原型 createProject title）
            let (_, card_rect) = find("先写下你的研究想法");
            assert!(
                card_rect.min.x > canvas_right,
                "{win_w}×{win_h}：产物卡应在对话栏区（x={}，画布右缘 {canvas_right}）",
                card_rect.min.x
            );

            // 4) 指标卡三段文字纵向堆叠（修复前在 horizontal 父级下横排同行）
            let (_, top_rect) = find("研究阶段");
            let (_, bottom_rect) = find("逐步构建研究证据");
            assert!(
                bottom_rect.top() > top_rect.top() + 5.0,
                "{win_w}×{win_h}：指标卡应纵向堆叠（顶 {} 底 {}）",
                top_rect.top(),
                bottom_rect.top()
            );
            assert!(
                (bottom_rect.min.x - top_rect.min.x).abs() < 20.0,
                "{win_w}×{win_h}：指标卡应同列（顶 x={} 底 x={}）",
                top_rect.min.x,
                bottom_rect.min.x
            );

            // 4b) 指标卡不得伸入对话栏（错误预乘白底时第 2/3 卡会画进右栏）
            let (_, ver_rect) = find("策略版本");
            assert!(
                ver_rect.max.x + 8.0 < canvas_right,
                "{win_w}×{win_h}：「策略版本」应留在画布内（右端 {}，画布右缘 {canvas_right}）",
                ver_rect.max.x
            );
            assert!(
                find("需求文档").1.width() >= 40.0,
                "{win_w}×{win_h}：指标卡主值「需求文档」应可见（白底上浅色字会被吃掉）"
            );

            // 4c) 「演示环境」在顶栏行，不与对话栏标题同排
            let (_, demo_rect) = find("演示环境");
            assert!(
                demo_rect.center().y < body_top + 4.0,
                "{win_w}×{win_h}：演示环境应在顶栏（cy={}，body 顶 {body_top}）",
                demo_rect.center().y
            );

            // 5) 无文本绘制越出窗口右缘（修复前对话内容溢出窗外，30px 字宽容差）
            for (t, rect) in &texts {
                assert!(
                    rect.max.x <= win_w + 30.0,
                    "{win_w}×{win_h}：文本「{}」越出窗口右缘（右端 {} > {}）",
                    t.chars().take(12).collect::<String>(),
                    rect.max.x,
                    win_w + 30.0
                );
            }
        }
    });
}

/// ST-S15-07：需求文档页无头渲染对齐 core-05 `#requirements`（页头徽章、默认草稿、「下载文档」、万元字段）。
#[test]
fn st_s15_07_requirements_page_copy() {
    case("ST-S15-07", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.session.navigate(Route::Requirements, 0.0);
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("研究需求"), "应有页标题");
        assert!(has("未确认草稿"), "未确认时应有页头徽章");
        assert!(has("下载文档"), "应有原型「下载文档」按钮");
        assert!(has("最低成交额（万元）"), "字段单位应对齐原型万元");
        assert!(has("日线量价信号"), "默认草稿正文应对齐原型 reqText");
        assert!(has("确认需求"), "应有确认主按钮");
    });
}

/// ST-S15-08：策略开发页无头渲染对齐 core-05 `#develop`。
#[test]
fn st_s15_08_develop_page_copy() {
    case("ST-S15-08", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.session.navigate(Route::Develop, 0.0);
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("策略开发"));
        assert!(has("未保存"));
        assert!(has("策略示例.py"));
        assert!(has("检查草稿"));
        assert!(has("生成初始代码"));
        assert!(has("资金约束修复"));
        assert!(has("generate_targets"));
        assert!(has("草稿待检查"));
    });
}

/// ST-S15-09：空态回测 / 验证 / 计划 / 策略资产 / 调试 页头与 empty 对齐 core-05。
#[test]
fn st_s15_09_empty_workspace_routes_copy() {
    case("ST-S15-09", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;

        fn texts_on(route: Route) -> Vec<String> {
            let ctx = egui::Context::default();
            let mut app = ResearchApp::new(false);
            app.session.navigate(route, 0.0);
            let mut shapes = Vec::new();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                };
                let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
                shapes = std::mem::take(&mut full.shapes);
                full.drop_without_applying_deltas();
            }
            shapes
                .iter()
                .filter_map(|cs| match &cs.shape {
                    egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                    _ => None,
                })
                .collect()
        }
        let has = |ts: &[String], n: &str| ts.iter().any(|t| t.contains(n));

        let exp = texts_on(Route::Experiments);
        assert!(has(&exp, "回测实验"));
        assert!(has(&exp, "准备你的第一次实验"));
        assert!(has(&exp, "运行合成实验"));
        assert!(
            !has(&exp, "运行参数"),
            "空态应对齐原型 empty，不展示协调器运行参数面板"
        );

        let val = texts_on(Route::Validate);
        assert!(has(&val, "策略验证"));
        assert!(has(&val, "生成验证报告"));
        assert!(has(&val, "尚未形成验证结论"));

        let plan = texts_on(Route::Plan);
        assert!(has(&plan, "交易计划"));
        assert!(has(&plan, "人工参考 · 演示"));
        assert!(has(&plan, "参考数据快照"));
        assert!(has(&plan, "计划交易日"));
        assert!(has(&plan, "可用现金（元）"));
        assert!(has(&plan, "SYN-A · 当前持仓"));
        assert!(has(&plan, "生成计划草稿"));
        assert!(!has(&plan, "手工持仓 JSON"));

        let lib = texts_on(Route::Library);
        assert!(has(&lib, "策略资产"));
        assert!(has(&lib, "打开编辑器"));
        assert!(has(&lib, "尚无策略版本"));

        let dbg = texts_on(Route::Debug);
        assert!(has(&dbg, "事件调试"));
        assert!(has(&dbg, "沿着一笔信号与订单"));
        assert!(has(&dbg, "还没有运行事件"));

        let design = texts_on(Route::Design);
        assert!(has(&design, "策略设计"));
        assert!(has(&design, "先明确需求，再设计处理逻辑"));
        assert!(has(&design, "等待确认需求"));
        assert!(has(&exp, "↵ 发送"));
    });
}

/// ST-S15-10：确认需求后策略设计「时序图」对齐 core-05 `sequenceSvg`。
#[test]
fn st_s15_10_design_sequence_diagram() {
    case("ST-S15-10", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;
        use nautilus_research_desktop::workspace::{
            DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, DRAFT_REQ_TEXT,
        };

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.workspace
            .confirm_requirement(DRAFT_REQ_TEXT, DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, 1000.0)
            .expect("确认需求");
        app.session.navigate(Route::Design, 0.0);
        app.design_tab_flow = false;
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 1600.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("2026-01-05 · 收盘形成信号"));
        assert!(has("2026-01-06 · 开盘执行"));
        assert!(has("① 可见行情"));
        assert!(has("② 入选样本"));
        assert!(has("③ 目标比例"));
        assert!(has("④ 资金约束"));
        assert!(has("⑤ 成交回报"));
        assert!(has("读取数据"));
        assert!(has("计算指标"));
        assert!(has("定位示例源码第"));
        assert!(has("设计参数"));
        assert!(!has("现金与持仓按当日收盘估值"), "不应再使用键值列表凑时序");
    });
}

/// ST-S15-11：交易计划输入区对齐 core-05 `#plan`（字段表单，非 JSON）。
#[test]
fn st_s15_11_plan_input_form() {
    case("ST-S15-11", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.session.navigate(Route::Plan, 0.0);
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 1600.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("参考数据快照"));
        assert!(has("01-08 收盘 · 合成计划快照"));
        assert!(has("计划交易日"));
        assert!(has("可用现金（元）"));
        assert!(has("SYN-A · 当前持仓"));
        assert!(has("SYN-B · 当前持仓"));
        assert!(has("SYN-C · 当前持仓"));
        assert!(has("可卖数量"));
        assert!(has("生成计划草稿"));
        assert!(has("计划核对不是回测"));
        assert!(!has("手工持仓 JSON"));
    });
}

/// ST-S15-12：有实验后回测页指标与记录对齐 core-05 `#experiments` 填充态。
#[test]
fn st_s15_12_experiments_filled_metrics() {
    case("ST-S15-12", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;
        use nautilus_research_desktop::workspace::{
            DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, DRAFT_REQ_TEXT,
        };

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.workspace
            .confirm_requirement(DRAFT_REQ_TEXT, DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, 1000.0)
            .expect("确认需求");
        app.workspace
            .generate_design_from_req("合成样本处理逻辑")
            .expect("保存设计");
        app.workspace
            .save_version("fn strategy() {}")
            .expect("保存版本");
        let eid = app
            .workspace
            .run_experiment(Some("T-demo".into()))
            .expect("运行实验");
        app.workspace
            .record_experiment(eid, Some("0.0625".into()));
        app.session.navigate(Route::Experiments, 0.0);
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 1600.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("E1 期末净值"));
        assert!(has("初始 10,000.00 元"));
        assert!(has("累计收益"));
        assert!(has("仅合成样本 · 不年化"));
        assert!(has("最大回撤"));
        assert!(has("三个估值点"));
        assert!(has("净值比较"));
        assert!(has("实验 / 版本"));
        assert!(has("首个事件分歧"));
        assert!(has("合成数据 · 非真实回测"));
        assert!(has("生成当前版本验证报告"));
        assert!(!has("最近收益"));
    });
}

/// ST-S15-13：有实验后事件调试页对齐 core-05 `#debug` 填充态信息架构。
#[test]
fn st_s15_13_debug_filled_playback() {
    case("ST-S15-13", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;
        use nautilus_research_desktop::workspace::{
            DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, DRAFT_REQ_TEXT,
        };

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.workspace
            .confirm_requirement(DRAFT_REQ_TEXT, DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, 1000.0)
            .expect("确认需求");
        app.workspace
            .generate_design_from_req("合成样本处理逻辑")
            .expect("保存设计");
        app.workspace
            .save_version("fn strategy() {}")
            .expect("保存版本");
        let eid = app
            .workspace
            .run_experiment(Some("T-demo".into()))
            .expect("运行实验");
        app.workspace
            .record_experiment(eid, Some("0.0625".into()));
        app.session.navigate(Route::Debug, 0.0);
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 1600.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("回放冻结实验"));
        assert!(has("事件回放"));
        assert!(has("全部事件"));
        assert!(has("回到起点"));
        assert!(has("单步 →"));
        assert!(has("节点 / 标的"));
        assert!(has("播放已记录的事件"));
        assert!(has("事件检查器"));
        assert!(has("当前筛选下没有事件"));
        assert!(has("这次运行告诉我们什么"));
        assert!(has("查看修复差异"));
        assert!(!has("当前版本与运行"));
        assert!(!has("逐事件回放属于后续生产能力"));
    });
}

/// ST-S15-14：预置示例运行后事件调试页有逐步事件与回放控件（对齐 core-05 `#debug`）。
#[test]
fn st_s15_14_debug_demo_events() {
    case("ST-S15-14", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;
        use nautilus_research_desktop::workspace::{
            DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, DRAFT_CODE_ORIGINAL, DRAFT_DESIGN_NOTE,
            DRAFT_REQ_TEXT,
        };

        let ctx = egui::Context::default();
        let mut app = ResearchApp::new(false);
        app.workspace
            .confirm_requirement(DRAFT_REQ_TEXT, DRAFT_ACCEPTANCE, DRAFT_ALLOC_PCT, 1000.0)
            .expect("确认需求");
        app.workspace
            .generate_design_from_req(DRAFT_DESIGN_NOTE)
            .expect("保存设计");
        app.workspace
            .save_version(DRAFT_CODE_ORIGINAL)
            .expect("保存原始示例");
        app.workspace
            .run_experiment(None)
            .expect("运行合成实验");
        app.session.navigate(Route::Debug, 0.0);
        // 筛选成交节点，确保拒绝事件进入可见区与检查器
        app.debug_filter = "fill".into();
        app.debug_event_index = 0;
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 1600.0),
                )),
                ..Default::default()
            };
            let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
            shapes = std::mem::take(&mut full.shapes);
            full.drop_without_applying_deltas();
        }
        let texts: Vec<String> = shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                _ => None,
            })
            .collect();
        let has = |n: &str| texts.iter().any(|t| t.contains(n));
        assert!(has("事件回放"));
        assert!(has("订单拒绝"));
        assert!(has("资金不足"));
        assert!(has("事件检查器"));
        assert!(has("定位冻结源码"));
        assert!(has("输入"));
        assert!(has("输出"));
        assert!(!has("当前筛选下没有事件"));
    });
}

/// ST-S15-15：数据中心 / 股票池演示层文案对齐 core-05 `#data` / `#pool`。
#[test]
fn st_s15_15_data_pool_demo_copy() {
    case("ST-S15-15", || {
        use nautilus_research_desktop::app::ResearchApp;
        use nautilus_research_desktop::nav::Route;

        fn texts_on(route: Route) -> Vec<String> {
            let ctx = egui::Context::default();
            let mut app = ResearchApp::new(false);
            app.session.navigate(route, 0.0);
            let mut shapes = Vec::new();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    ..Default::default()
                };
                let mut full = ctx.run_ui(input, |ui| app.render_root(ui));
                shapes = std::mem::take(&mut full.shapes);
                full.drop_without_applying_deltas();
            }
            shapes
                .iter()
                .filter_map(|cs| match &cs.shape {
                    egui::Shape::Text(ts) => Some(ts.galley.text().to_string()),
                    _ => None,
                })
                .collect()
        }
        let has = |ts: &[String], n: &str| ts.iter().any(|t| t.contains(n));

        let data = texts_on(Route::Data);
        assert!(has(&data, "数据中心"));
        assert!(has(&data, "模拟更新快照"));
        assert!(has(&data, "当前合成数据快照"));
        assert!(has(&data, "价格与信号样本"));
        assert!(has(&data, "快照编号"));
        assert!(has(&data, "SYN-202601-r1"));
        assert!(has(&data, "信号收盘"));

        let pool = texts_on(Route::Pool);
        assert!(has(&pool, "股票池"));
        assert!(has(&pool, "保存股票池规则"));
        assert!(has(&pool, "候选样本"));
        assert!(has(&pool, "满足流动性"));
        assert!(has(&pool, "搜索样本代码或名称"));
        assert!(has(&pool, "U1"));
    });
}
