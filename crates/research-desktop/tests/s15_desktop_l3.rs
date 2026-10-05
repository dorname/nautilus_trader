//! S15 桌面批次 L3（流水线五页对接）场景测试：页面状态机 + 协调器桥全链路。
//!
//! 规格对齐：logos/resources/test/core-S15-test-cases.md（UT-S15-09、ST-S15-04）。
//! 测试不经图形后端：以 DesktopBridge + TaskWatch 状态机驱动真实协调器
//! （进程内执行器，temp workspace），与 GUI 渲染路径共享同一套模型函数。

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
