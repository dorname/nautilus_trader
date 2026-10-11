//! M4 验收测试：S19 实验派发与验证证据链。
//!
//! CPU 红线：实验派发为单任务，无并发压力；
//! 验证证据链通过协调器真实计算，不由 LLM 生成。

#[path = "support/profile.rs"]
mod test_profile;

use nautilus_research_desktop::agent_bridge::{AgentBridge, AgentSessionConfig};
use nautilus_research_desktop::bridge::{DesktopBridge, ImportForm, UniverseForm};
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_desktop::pipeline::{TaskWatch, is_success};
use nautilus_research_desktop::swarm_bridge::{
    SwarmBridge, SwarmDispatchState, SubtaskState, build_run_form, parse_run_params,
    s19_experiment_contracts, s19_organize_params_prompt,
};
use nautilus_research_desktop::workspace::Workspace;
use nautilus_research_testkit::case;

/// 轮询等真实终态（turn/completed 事件流口径），超时 panic。
/// embed-octos-runtime：调用方预算由 300s 放宽（见各调用点注释）。
fn wait_terminal(swarm: &mut SwarmBridge, timeout_secs: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while swarm.is_active() {
        assert!(
            std::time::Instant::now() < deadline,
            "{timeout_secs}s 内未达终态"
        );
        swarm.poll_all().expect("poll_all");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn temp_agent_config(tag: &str) -> AgentSessionConfig {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "rd-m4-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let data_dir = base.join("data");
    let instance_dir = base.join("instance");

    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&instance_dir).expect("create instance dir");

    // issue #24：经 HOME 解析，禁止机器绝对路径硬编码
    let profile_src = test_profile::configured_source();
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if let Some(profile_src) = profile_src {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("m4-{tag}"),
        profile_id: "octos".into(),
        profile_warning: None,
        oup: OupConfig {
            octos_bin: "octos".into(),
            data_dir: data_dir.to_string_lossy().into(),
            instance_dir: instance_dir.to_string_lossy().into(),
            // issue #24：禁止机器绝对路径；仓库根经 CARGO_MANIFEST_DIR 解析
            workspace_root: concat!(env!("CARGO_MANIFEST_DIR"), "/../..").into(),
            extra_env: Vec::new(),
            danger_full_access: false,
        },
    }
}

/// ST-M4-01：S19 实验契约派发。
#[test]
fn st_m4_01_experiment_dispatch() {
    let config = temp_agent_config("experiment");
    let mut swarm = SwarmBridge::new();

    let contracts = s19_experiment_contracts("v1", "snapshot-20240101");
    assert_eq!(contracts.len(), 1);
    assert_eq!(contracts[0].contract_id, "experiment");
    assert!(contracts[0].prompt.contains("v1"));
    assert!(contracts[0].prompt.contains("snapshot-20240101"));
    assert!(contracts[0].prompt.contains("协调器计算任务"));

    let turn_ids = swarm
        .dispatch_parallel(&config, contracts)
        .expect("dispatch experiment");

    assert_eq!(turn_ids.len(), 1);
    assert!(swarm.is_active());

    // 等真实终态（issue #12 修复口径）：Succeeded 且 output 非空；
    // 实验 prompt 为重量级产出，LLM 尾延迟时段放宽到 300s
    // embed-octos-runtime：300s 在 verify 全套（前置 m2/m3 已耗时、API 拥挤尾延迟
// 实测 631s 仍未达终态而单跑 131s 通过）下误杀，放宽到 600s 吸收波动。
    wait_terminal(&mut swarm, 600);
    assert_eq!(swarm.state(), SwarmDispatchState::Completed);
    let r = &swarm.results()[0];
    assert_eq!(r.state, SubtaskState::Succeeded, "error: {:?}", r.error);
    assert!(
        r.output.as_deref().is_some_and(|s| !s.trim().is_empty()),
        "实验 output 应非空"
    );

    swarm.disconnect_all();
}

/// ST-M4-02：实验契约包含版本和数据快照引用。
#[test]
fn st_m4_02_experiment_version_freeze() {
    let config = temp_agent_config("freeze");
    let mut swarm = SwarmBridge::new();

    // 模拟冻结版本引用
    let contracts = s19_experiment_contracts("v2", "snapshot-20240115");
    swarm
        .dispatch_parallel(&config, contracts)
        .expect("dispatch frozen version");

    assert!(swarm.is_active());
    assert_eq!(swarm.results()[0].contract_id, "experiment");

    // 等真实终态，冻结版本引用的实验同样需产出非空文本
    // embed-octos-runtime：300s 在 verify 全套（前置 m2/m3 已耗时、API 拥挤尾延迟
// 实测 631s 仍未达终态而单跑 131s 通过）下误杀，放宽到 600s 吸收波动。
    wait_terminal(&mut swarm, 600);
    assert_eq!(swarm.state(), SwarmDispatchState::Completed);
    assert!(
        swarm.results()[0]
            .output
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty()),
        "实验 output 应非空"
    );

    swarm.disconnect_all();
}

/// ST-M4-03：空版本 ID 契约生成（由协调器校验）。
#[test]
fn st_m4_03_empty_version_rejected() {
    let contracts = s19_experiment_contracts("", "snapshot");
    // 契约生成不拒绝空版本（由协调器校验）
    assert_eq!(contracts.len(), 1);
    assert!(contracts[0].prompt.contains("冻结版本"));
}

/// ST-M4-04：实验契约包含验证边界声明。
#[test]
fn st_m4_04_experiment_evidence_boundary() {
    let contracts = s19_experiment_contracts("v1", "snap");
    // 验证 prompt 中包含"数值结果必须来自协调器"的约束
    assert!(contracts[0].prompt.contains("数值结果必须来自协调器计算任务"));
    assert!(contracts[0].prompt.contains("不得自行生成"));
}

/// UT-S19-16：parse_run_params 判定矩阵——合法 / fence 包裹 / 非 JSON / 缺字段 /
/// fast>=slow / top_k=0 / 非正金额 / 非法日期 / start>end / 结果数值字段。
#[test]
fn ut_s19_16_parse_run_params_matrix() {
    case("UT-S19-16", || {
        let valid = r#"{"fast":1,"slow":2,"top_k":1,"start":"2023-12-04","end":"2024-01-05","capital":"20000"}"#;
        let p = parse_run_params(valid).expect("合法 JSON 应解析成功");
        assert_eq!(p.fast, 1);
        assert_eq!(p.slow, 2);
        assert_eq!(p.top_k, 1);
        assert_eq!(p.start, "2023-12-04");
        assert_eq!(p.end, "2024-01-05");
        assert_eq!(p.capital, "20000");

        // fence 包裹 + 数字型 capital
        let fenced = format!("参数如下：\n```json\n{valid}\n```\n以上。");
        let p2 = parse_run_params(&fenced).expect("fence 包裹应解析成功");
        assert_eq!(p2, p);

        // build_run_form 只映射表单参数
        let form = build_run_form(&p);
        assert_eq!(form.fast, 1);
        assert_eq!(form.slow, 2);
        assert_eq!(form.top_k, 1);
        assert_eq!(form.start, "2023-12-04");
        assert_eq!(form.end, "2024-01-05");
        assert_eq!(form.capital, "20000");

        // 非 JSON
        assert!(parse_run_params("我无法整理参数").is_err());
        // 缺字段
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":1,"start":"2023-12-04","end":"2024-01-05"}"#)
            .expect_err("缺 capital 应拒绝")
            .contains("capital"));
        // fast>=slow
        assert!(parse_run_params(r#"{"fast":5,"slow":5,"top_k":1,"start":"2023-12-04","end":"2024-01-05","capital":"20000"}"#)
            .expect_err("fast>=slow 应拒绝")
            .contains("须小于"));
        // top_k=0
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":0,"start":"2023-12-04","end":"2024-01-05","capital":"20000"}"#)
            .expect_err("top_k=0 应拒绝")
            .contains("top_k"));
        // 非正金额
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":1,"start":"2023-12-04","end":"2024-01-05","capital":"-100"}"#)
            .expect_err("负 capital 应拒绝")
            .contains("正数"));
        // 非法日期
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":1,"start":"2023-13-04","end":"2024-01-05","capital":"20000"}"#)
            .expect_err("月份越界应拒绝")
            .contains("start"));
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":1,"start":"20231204","end":"2024-01-05","capital":"20000"}"#)
            .expect_err("日期格式非法应拒绝")
            .contains("YYYY-MM-DD"));
        // start > end
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":1,"start":"2024-01-05","end":"2023-12-04","capital":"20000"}"#)
            .expect_err("start>end 应拒绝")
            .contains("不得晚于"));
        // 结果数值字段（实验数值只来自协调器，LLM 不得输出）
        assert!(parse_run_params(r#"{"fast":1,"slow":2,"top_k":1,"start":"2023-12-04","end":"2024-01-05","capital":"20000","total_return":"0.06"}"#)
            .expect_err("含结果数值字段应拒绝")
            .contains("total_return"));
    });
}

// ---------------------------------------------------------------- ST-S19-15 夹具（真实协调器）

fn temp_workspace(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "rd-m4-ws-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

/// 24 个交易日行情（2023-12-04 起，收盘 10..33 阶梯上行；对齐 s17_s20_ai 夹具）。
fn rising_bars(code: &str, dir: &std::path::Path, name: &str) -> String {
    let dates = [
        "2023-12-04", "2023-12-05", "2023-12-06", "2023-12-07", "2023-12-08",
        "2023-12-11", "2023-12-12", "2023-12-13", "2023-12-14", "2023-12-15",
        "2023-12-18", "2023-12-19", "2023-12-20", "2023-12-21", "2023-12-22",
        "2023-12-25", "2023-12-26", "2023-12-27", "2023-12-28", "2023-12-29",
        "2024-01-02", "2024-01-03", "2024-01-04", "2024-01-05",
    ];
    let mut body =
        String::from("instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n");
    for (i, d) in dates.iter().enumerate() {
        let close = 10 + i as i32;
        body.push_str(&format!(
            "{code},{d},{close},{close},{close},{close},100000,{close}0000\n"
        ));
    }
    let path = dir.join(name);
    std::fs::write(&path, body).expect("写入行情 CSV");
    path.to_string_lossy().to_string()
}

const RULE_ALL_PASS: &str = r#"{"op":"and","children":[{"field":"close","op":"gte","value":"0"}]}"#;

/// GUI 形态的等待：get_task + TaskWatch::poll 推进到终态。
fn watch_to_terminal(bridge: &DesktopBridge, watch: &mut TaskWatch) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
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
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// ST-S19-15：真实 LLM 整理参数 → parse_run_params → build_run_form → 真实协调器
/// 跑通——实验记录绑定真实 task_id，净值数值来自协调器 equity 而非 LLM 文本。
///
/// 真实 LLM 调用产生 API 费用（prompt 已最小化，单 turn）；协调器使用合成数据
/// 临时工作区（对齐 s17_s20_ai ST-S17-16 夹具口径）。
#[test]
fn st_s19_15_llm_params_to_real_coordinator() {
    case("ST-S19-15", || {
        // —— 真实协调器数据准备：导入 → 预览 → 保存股票池 ——
        let ws = temp_workspace("st15");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let bridge = DesktopBridge::open(ws.clone()).expect("打开工作区");
        let mut watch = {
            let form = ImportForm {
                source: 0,
                paths: vec![quotes],
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
            let r = bridge.submit_preview(&snapshot_id, &form).expect("预览提交");
            TaskWatch::submitted(r.task_id, "预览中…")
        };
        assert!(watch_to_terminal(&bridge, &mut watch), "预览应到终态");
        let preview_task = watch.task_id().expect("预览任务 ID").to_string();
        let p = bridge.preview(&preview_task).expect("预览结果");
        let universe = bridge
            .save_universe(&preview_task, &p.preview_hash, &p.input_hash, "ST-S19-15")
            .expect("保存股票池");

        // —— 真实 LLM 整理参数（最小化单 turn；需求文本含数据区间强约束）——
        let mut agent = AgentBridge::new(temp_agent_config("organize"));
        agent.connect().expect("连接 octos");
        let prompt = s19_organize_params_prompt(
            "单标的均线策略研究：持仓 1 只，数据区间 2023-12-04 至 2024-01-05，初始资金 20000 元",
            "EMA 双均线交叉，fast=1，slow=2",
        );
        let (turn_id, outcome) = agent.submit_and_wait(&prompt, 420).expect("LLM turn");
        assert!(
            uuid::Uuid::parse_str(&turn_id).is_ok(),
            "turn_id 应为合法 UUID: {turn_id}"
        );
        let reply = outcome.assistant_text.unwrap_or_default();
        assert!(!reply.trim().is_empty(), "LLM 回复应非空");
        agent.disconnect();

        // —— 严格校验：解析成功且只含表单参数 ——
        let params = parse_run_params(&reply)
            .unwrap_or_else(|e| panic!("LLM 参数应通过校验：{e}；回复：{reply}"));
        let mut form = build_run_form(&params);
        // 费用/有效期为环境字段（进程内执行器确定性快通道），非 LLM 整理范围
        form.commission_rate = "0".into();
        form.min_commission = "0".into();
        form.sell_tax = "0".into();
        form.other_fee = "0".into();
        form.slippage_bps = "0".into();
        form.participation = "1".into();
        form.eff_start = "2023-12-01".into();
        form.eff_end = "2024-02-01".into();

        // —— 真实协调器执行：任务终态成功 ——
        let r = bridge
            .submit_run(&snapshot_id, &universe.universe_id, &form)
            .expect("运行提交");
        let task_id = r.task_id.clone();
        let mut watch = TaskWatch::submitted(r.task_id, "实验运行中…");
        assert!(watch_to_terminal(&bridge, &mut watch), "实验应到终态");
        assert!(
            is_success(watch.terminal().expect("终态视图")),
            "实验终态应成功"
        );

        // —— 实验记录绑定真实 task_id；净值数值来自协调器 equity ——
        let mut w = Workspace::new();
        w.confirm_requirement("单标的均线策略研究", "期末收益>0", 30.0, 1_000.0)
            .expect("确认需求");
        w.generate_design_from_req("EMA 双均线").expect("生成设计");
        w.save_version("fn strategy() {}").expect("保存版本");
        let e1 = w.run_experiment(None).expect("实验冻结");
        w.attach_task(e1, task_id.clone());
        assert_eq!(
            w.current().experiments[e1 - 1].task_id.as_deref(),
            Some(task_id.as_str()),
            "实验记录应绑定真实 task_id"
        );
        let equity = bridge.equity_curve(&task_id).expect("读取净值曲线");
        assert!(!equity.is_empty(), "净值曲线应非空（数值来自协调器）");
        assert!(
            equity.iter().all(|pt| pt.value.is_finite() && pt.value > 0.0),
            "净值应为协调器计算的有限正数"
        );
    });
}
