//! S17～S20 AI 工作台（批次 L4）场景测试：意图路由、项目隔离、版本冻结、
//! 上游过期、证据边界、计划核对与确认导出；全旅程对接真实协调器。
//!
//! 规格对齐：logos/resources/test/core-S17~S20-test-cases.md「GUI 落地口径」段
//! （可自动化部分由 research-desktop UT/ST 承载接 reporter；渲染/焦点旅程保持
//! [manual]，ST-S17-11~15 / ST-S18-11~14 / ST-S19-11~14 / ST-S20-11~13 不变）。

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nautilus_research_desktop::ai::{self, Intent};
use nautilus_research_desktop::bridge::{DesktopBridge, ImportForm, RunForm, UniverseForm};
use nautilus_research_desktop::nav::Route;
use nautilus_research_desktop::pipeline::{TaskWatch, is_success};
use nautilus_research_desktop::workspace::{Role, Workspace, plan_signature};
use nautilus_research_testkit::case;

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "research-s17-20-{tag}-{}-{nanos}",
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

/// 零费率 EMA1/2 运行表单（进程内执行器确定性快通道）。
fn run_form_zero_cost() -> RunForm {
    RunForm {
        fast: 1,
        slow: 2,
        top_k: 1,
        start: "2023-12-04".into(),
        end: "2024-01-05".into(),
        capital: "20000".into(),
        commission_rate: "0".into(),
        min_commission: "0".into(),
        sell_tax: "0".into(),
        other_fee: "0".into(),
        slippage_bps: "0".into(),
        participation: "1".into(),
        eff_start: "2023-12-01".into(),
        eff_end: "2024-02-01".into(),
    }
}

/// UT-S17-16：离线意图路由（含未知诚实）＋项目隔离＋需求确认校验。
#[test]
fn ut_s17_16_intent_routing_and_project_isolation() {
    case("UT-S17-16", || {
        // —— 意图路由：预设意图全覆盖 + 未知诚实拒绝 ——
        assert_eq!(route_or_panic("请帮我确认需求"), Intent::ConfirmRequirement);
        assert_eq!(route_or_panic("新建一个项目"), Intent::NewProject);
        assert_eq!(route_or_panic("切换项目"), Intent::SwitchProject);
        assert_eq!(route_or_panic("生成设计说明"), Intent::GenerateDesign);
        assert_eq!(route_or_panic("保存版本 v2"), Intent::SaveVersion);
        assert_eq!(route_or_panic("跑实验"), Intent::RunExperiment);
        assert_eq!(route_or_panic("比较两次实验"), Intent::CompareExperiments);
        assert_eq!(route_or_panic("生成验证报告"), Intent::MakeReport);
        assert_eq!(route_or_panic("生成计划"), Intent::PlanGenerate);
        assert_eq!(route_or_panic("核对计划"), Intent::PlanCheck);
        assert_eq!(route_or_panic("导出 CSV"), Intent::PlanExport);
        assert_eq!(route_or_panic("明天买什么股票"), Intent::Unknown);
        assert_eq!(route_or_panic(""), Intent::Unknown);
        // 未知意图不编造：回复解释能力边界
        assert!(ai::reply("明天买什么股票").contains("不能理解"));
        // 意图 → 统一工作台路由（对话跳转目标）
        assert_eq!(
            Route::from_intent(Intent::ConfirmRequirement),
            Some(Route::Requirements)
        );
        assert_eq!(
            Route::from_intent(Intent::SaveVersion),
            Some(Route::Develop)
        );
        assert_eq!(
            Route::from_intent(Intent::RunExperiment),
            Some(Route::Experiments)
        );
        assert_eq!(Route::from_intent(Intent::PlanExport), Some(Route::Plan));
        assert_eq!(Route::from_intent(Intent::Unknown), None);

        // —— 项目隔离：新项目会话/需求/版本独立；切回保留 ——
        let mut w = Workspace::new();
        assert_eq!(
            w.confirm_requirement("量价", "夏普>0", 30.0, 100.0)
                .unwrap(),
            1
        );
        w.add_project("对照项目");
        assert!(w.current().reqs.is_empty());
        assert!(w.switch_to(9) == false, "越界切换保持不变");
        assert!(w.switch_to(0));
        assert_eq!(w.current().reqs.len(), 1);
        assert!(
            w.current()
                .messages
                .iter()
                .any(|m| m.role == Role::Assistant)
        );

        // —— 需求确认校验：空正文 / 越界 / 负成交额拒绝，合法生成 R 版本 ——
        assert_eq!(
            w.confirm_requirement(" ", "x", 10.0, 0.0).unwrap_err(),
            "请填写研究目标"
        );
        assert_eq!(
            w.confirm_requirement("t", " ", 10.0, 0.0).unwrap_err(),
            "请填写验收标准"
        );
        assert_eq!(
            w.confirm_requirement("t", "a", 100.01, 0.0).unwrap_err(),
            "投入比例须为 0～100%"
        );
        assert_eq!(
            w.confirm_requirement("t", "a", 0.0, -0.01).unwrap_err(),
            "成交额须为非负数"
        );
        let r1 = w
            .confirm_requirement("量价选股", "夏普>0", 30.0, 1_000_000.0)
            .unwrap();
        assert_eq!(r1, 2, "同项目内需求版本顺延编号");
        assert!((w.current().reqs[1].allocation - 0.3).abs() < 1e-9);
    });
}

fn route_or_panic(text: &str) -> Intent {
    let i = ai::route(text);
    assert!(ai::reply(text).len() > 8, "每个输入都有预设回复：{text}");
    i
}

/// UT-S18-15：版本冻结与上游过期（不可变、引用冻结、过期拒绝、历史不变、幂等）。
#[test]
fn ut_s18_15_version_freeze_and_staleness() {
    case("UT-S18-15", || {
        let mut w = Workspace::new();
        // 无需求时生成设计拒绝；确认后设计绑定 R1
        assert!(w.generate_design_from_req("无需求设计").is_err());
        w.confirm_requirement("量价", "夏普>0", 30.0, 100.0)
            .unwrap();
        let d1 = w.generate_design_from_req("EMA 双均线").unwrap();
        let v1 = w.save_version("fn strategy() {}").unwrap();
        // 版本冻结 R1/D1 与保存时刻修订计数
        {
            let p = w.current();
            let v = &p.versions[v1 - 1];
            assert_eq!((v.req_id, v.design_id), (1, d1));
        }
        assert!(w.version_fresh(v1), "刚保存的版本新鲜");
        // 不可变：同源码幂等返回原版本，不产生新冻结戳
        assert_eq!(w.save_version("fn strategy() {}").unwrap(), v1);
        assert_eq!(w.current().versions.len(), 1);

        // 上游更新（确认新需求）→ 旧版本过期：不可新运行，历史实验不变
        let e1 = w.run_experiment(Some("T0".into())).unwrap();
        w.record_experiment(e1, Some("0.0625".into()));
        w.confirm_requirement("量价 v2", "夏普>0.5", 40.0, 100.0)
            .unwrap();
        assert!(!w.version_fresh(v1));
        assert_eq!(
            w.run_experiment(None).unwrap_err(),
            "版本未保存或上游已过期，请保存当前版本后再运行"
        );
        assert_eq!(
            w.current().experiments[0].total_return.as_deref(),
            Some("0.0625")
        );
        // 恢复冻结引用仍过期：冻结戳落后即过期
        w.current_mut().active_req = Some(1);
        assert!(!w.version_fresh(v1), "冻结戳落后即过期");
    });
}

/// UT-S19-15：证据边界（演示检查可过、正式验证恒「证据不足」）与实验比较归因。
#[test]
fn ut_s19_15_evidence_boundary_and_comparison() {
    case("UT-S19-15", || {
        let mut w = Workspace::new();
        w.confirm_requirement("量价", "夏普>0", 30.0, 100.0)
            .unwrap();
        w.generate_design_from_req("EMA").unwrap();
        w.save_version("fn v1() {}").unwrap();
        // 无实验 → 拒绝生成验证结论
        assert!(w.make_report().is_err());
        // 实验执行后：演示检查可过（执行证据 = 任务引用），正式验证恒证据不足
        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        let vid = w.make_report().unwrap();
        let report = w.current().report.as_ref().unwrap();
        assert_eq!(report.version_id, vid);
        assert_eq!(report.run_id, e1);
        assert!(report.demo_pass(), "演示检查通过（实验已执行）");
        let formal = report
            .checks
            .iter()
            .find(|(n, _, _)| n == "正式策略验证")
            .expect("报告含正式验证项");
        assert!(!formal.1, "正式验证恒不通过");
        assert!(formal.2.contains("证据不足"));

        // 比较归因：同输入（同版本同戳）分歧归因代码；跨输入列差异不归因代码
        let e2 = w.run_experiment(Some("T2".into())).unwrap();
        let (same, diffs) = w.compare_experiments(e1, e2).unwrap();
        assert!(same && diffs.is_empty(), "同版本同戳为同输入");
        w.confirm_requirement("量价 v2", "夏普>0.5", 40.0, 100.0)
            .unwrap();
        w.generate_design_from_req("EMA v2").unwrap();
        w.save_version("fn v2() {}").unwrap();
        let e3 = w.run_experiment(Some("T3".into())).unwrap();
        let (same, diffs) = w.compare_experiments(e1, e3).unwrap();
        assert!(!same);
        assert!(diffs.iter().any(|d| d.contains("代码版本")));
        assert!(diffs.iter().any(|d| d.contains("输入修订")));
    });
}

/// UT-S20-14：计划核对（恒等式/可卖/整手/现金/过期/报告）与确认导出门控。
#[test]
fn ut_s20_14_plan_check_and_gated_export() {
    case("UT-S20-14", || {
        let mut w = Workspace::new();
        w.confirm_requirement("量价", "夏普>0", 30.0, 100.0)
            .unwrap();
        w.generate_design_from_req("EMA").unwrap();
        w.save_version("fn v1() {}").unwrap();
        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        w.make_report().unwrap();
        assert_eq!(w.current().report.as_ref().unwrap().run_id, e1);
        let rows = || vec![("SYN-A".to_string(), 10.0, 1000, 1000, 1100, 100)];
        // 账户恒等式（现金+持仓市值=总资产）不成立 → 生成即拒绝
        assert!(
            w.make_plan("SNAP-1", "2024-01-05", 10000.0, 99999.0, rows())
                .is_err()
        );
        w.make_plan("SNAP-1", "2024-01-05", 10000.0, 20000.0, rows())
            .unwrap();
        let sig = w.current().plan.as_ref().unwrap().signature;

        // 未核对 → 导出拒绝；核对不过逐项暴露
        assert_eq!(w.confirm_export(sig).unwrap_err(), "计划未核对或核对未通过");
        let mut bad = w.clone();
        bad.current_mut().plan.as_mut().unwrap().rows = vec![
            ("SYN-A".into(), 10.0, 100, 50, 40, -60),
            ("SYN-B".into(), 9.0, 0, 0, 250, 250),
        ];
        let issues = bad.check_plan(5.0).unwrap();
        assert!(
            issues.iter().any(|i| i.contains("SYN-A 可卖数量不足")),
            "{issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|i| i.contains("SYN-B 买入数量不是 100 股整数倍"))
        );
        // 现金不足
        let issues = w.check_plan(999_999.0).unwrap();
        assert!(issues.iter().any(|i| i.contains("可用现金不足")));
        assert!(!w.current().plan.as_ref().unwrap().checked);
        // 报告缺失 → 核对暴露
        let mut no_report = w.clone();
        no_report.current_mut().report = None;
        assert!(
            no_report
                .check_plan(1500.0)
                .unwrap()
                .iter()
                .any(|i| i.contains("验证报告缺失"))
        );

        // 核对通过 → 确认导出：CSV 含演示标识/版本/交易日/调整数量
        assert!(w.check_plan(1500.0).unwrap().is_empty());
        assert!(w.current().plan.as_ref().unwrap().checked);
        let csv = w.confirm_export(sig).unwrap();
        assert!(csv.contains("演示标识"));
        assert!(csv.contains("v1"));
        assert!(csv.contains("2024-01-05"));
        assert!(csv.contains("SYN-A,10,100"));
        // 确认期间输入改变 → 签名失配，阻断导出
        assert_eq!(
            w.confirm_export(sig + 1).unwrap_err(),
            "确认期间输入已改变，已阻止导出"
        );
        // 表单输入与草稿不一致时重算签名同样失配
        let mismatch = plan_signature(1, "SNAP-1", "2024-01-04", 10000.0, &rows());
        assert!(w.confirm_export(mismatch).is_err());
    });
}

/// ST-S17-16：AI 工作台全旅程（长驻协调器）——意图对话 → 需求 → 设计 → 版本
/// → 真实实验（提交/取消）→ 上游过期 → 报告 → 计划核对 → 确认导出 CSV 落盘。
#[test]
fn st_s17_16_ai_workspace_full_journey() {
    case("ST-S17-16", || {
        let ws = temp_workspace("st16");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let bridge = DesktopBridge::open(ws.clone()).expect("打开工作区");

        // —— 数据准备：导入 → 预览 → 保存（实验与计划的协调器输入）——
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
        let snapshot_id = watch
            .terminal()
            .filter(|v| is_success(v))
            .and_then(|v| v.snapshot_id.clone())
            .expect("导入成功");
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
        let preview_task = watch.task_id().expect("预览任务 ID").to_string();
        let p = bridge.preview(&preview_task).expect("预览结果");
        let universe = bridge
            .save_universe(&preview_task, &p.preview_hash, &p.input_hash, "ST-S17-16")
            .expect("保存股票池");

        // —— 对话：意图路由（离线预设）——
        assert_eq!(ai::route("运行实验"), Intent::RunExperiment);
        assert_eq!(
            Route::from_intent(Intent::RunExperiment),
            Some(Route::Experiments)
        );

        // —— 项目：需求确认 → 设计 → 版本冻结 ——
        let mut w = Workspace::new();
        w.confirm_requirement("量价选股", "期末收益>0", 30.0, 1_000_000.0)
            .expect("确认需求");
        w.generate_design_from_req("EMA 双均线").expect("生成设计");
        let v1 = w.save_version("fn strategy() {}").expect("保存版本");
        assert!(w.version_fresh(v1));

        // —— 调试：真实协调器实验（冻结校验通过 → 提交 → 终态）——
        let e1 = w.run_experiment(None).expect("实验冻结");
        let r = bridge
            .submit_run(&snapshot_id, &universe.universe_id, &run_form_zero_cost())
            .expect("运行提交");
        let task_a = r.task_id.clone();
        let mut watch = TaskWatch::submitted(r.task_id, "AI 工作台实验运行中…");
        assert!(watch_to_terminal(&bridge, &mut watch), "实验应到终态");
        assert!(is_success(watch.terminal().expect("终态视图")));
        w.attach_task(e1, task_a.clone());
        // 报告：演示检查可过（实验已执行），正式验证恒证据不足
        w.make_report().expect("生成报告");
        let report = w.current().report.as_ref().unwrap();
        assert!(report.demo_pass());
        assert!(
            report
                .checks
                .iter()
                .any(|(n, ok, msg)| n == "正式策略验证" && !ok && msg.contains("证据不足"))
        );

        // —— 计划桥：生成 → 核对 → 确认导出 CSV 落盘（含演示标识）——
        let plan_json = r#"{"cash_cny":"10000","total_assets_cny":"20000","positions":[{"instrument_id":"SYN-A","price":10,"quantity":1000,"sellable_quantity":1000,"target_quantity":1100}]}"#;
        let (cash, total, rows) =
            nautilus_research_desktop::bridge::parse_plan_rows(plan_json).expect("持仓解析");
        w.make_plan(&snapshot_id, "2024-01-05", cash, total, rows)
            .expect("生成计划");
        let buys_cost = 10.0 * 100.0; // 100 股 × 参考价 10，零费率
        assert!(w.check_plan(buys_cost).expect("核对").is_empty());
        let sig = w.current().plan.as_ref().unwrap().signature;
        let csv = w.confirm_export(sig).expect("确认导出");
        assert!(csv.contains("演示标识") && csv.contains("SYN-A,10,100"));
        let export_path = ws.join("研序-交易计划.csv");
        std::fs::write(&export_path, csv.as_bytes()).expect("写导出文件");
        let written = std::fs::read_to_string(&export_path).expect("读回导出文件");
        assert_eq!(written, csv, "落盘内容与确认导出一致");

        // —— 上游过期：确认新需求后旧版本不可新运行，历史实验不变 ——
        let e1_task = w.current().experiments[0].task_id.clone();
        w.confirm_requirement("量价 v2", "期末收益>0.1", 40.0, 1_000_000.0)
            .expect("更新需求");
        assert!(!w.version_fresh(v1));
        assert!(w.run_experiment(None).is_err(), "过期版本拒绝新实验");
        assert_eq!(
            w.current().experiments[0].task_id.as_deref(),
            e1_task.as_deref()
        );

        // —— 新版本下取消旅程：长驻协调器取消直达（完成则 ALREADY_TERMINAL 容双态）——
        w.generate_design_from_req("EMA v2").expect("生成设计 v2");
        let v2 = w.save_version("fn strategy_v2() {}").expect("保存版本 v2");
        assert!(w.version_fresh(v2));
        let e2 = w.run_experiment(None).expect("实验 v2 冻结");
        let r = bridge
            .submit_run(&snapshot_id, &universe.universe_id, &run_form_zero_cost())
            .expect("运行提交 v2");
        let task_b = r.task_id.clone();
        let mut watch = TaskWatch::submitted(r.task_id, "AI 工作台实验运行中…");
        match bridge.cancel(&task_b) {
            Ok(v) => assert_eq!(v.state, nautilus_research_domain::TaskState::Cancelling),
            Err(e) => assert_eq!(
                serde_json::to_value(&e.code).unwrap(),
                serde_json::json!("ALREADY_TERMINAL"),
                "取消拒绝仅允许已终态：{e:?}"
            ),
        }
        assert!(watch_to_terminal(&bridge, &mut watch), "取消后应到终态");
        let v = watch.terminal().expect("终态视图").clone();
        assert!(
            is_success(&v) || v.state == nautilus_research_domain::TaskState::Cancelled,
            "终态与取消竞争一致：{v:?}"
        );
        // 取消/完成都如实记录；历史实验 E1 不受影响
        w.attach_task(e2, task_b);
        assert_eq!(w.current().experiments.len(), 2);
        assert_eq!(
            w.current().experiments[0].task_id.as_deref(),
            e1_task.as_deref(),
            "历史实验不变"
        );
    });
}
