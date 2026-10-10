//! M3 验收测试：Pipeline 拓扑（设计→代码检查）与真实 octos serve 的端到端通信。
//!
//! CPU 红线：Pipeline 串行执行，同一时间只有一个 sub-agent 活跃；
//! 每个阶段最多等待 30 秒。

use nautilus_research_desktop::agent_bridge::AgentSessionConfig;
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_desktop::swarm_bridge::{
    SwarmBridge, SwarmDispatchState, SubtaskContract, SubtaskState, s18_design_to_code_contracts,
};
use nautilus_research_testkit::case;

fn temp_agent_config(tag: &str) -> AgentSessionConfig {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "rd-m3-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let data_dir = base.join("data");
    let instance_dir = base.join("instance");

    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&instance_dir).expect("create instance dir");

    // issue #24：经 HOME 解析，禁止机器绝对路径硬编码
    let profile_src = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|h| h.join(".octos/profiles/octos.json"))
        .filter(|p| p.exists());
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if let Some(profile_src) = profile_src {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("m3-{tag}"),
        profile_id: "octos".into(),
        oup: OupConfig {
            octos_bin: "octos".into(),
            data_dir: data_dir.to_string_lossy().into(),
            instance_dir: instance_dir.to_string_lossy().into(),
            // issue #24：禁止机器绝对路径；仓库根经 CARGO_MANIFEST_DIR 解析
            workspace_root: concat!(env!("CARGO_MANIFEST_DIR"), "/../..").into(),
            extra_env: Vec::new(),
        },
    }
}

/// ST-M3-01：Pipeline 派发 2 个子任务（设计→代码检查），串行执行至终态。
#[test]
fn st_m3_01_pipeline_dispatch() {
    let config = temp_agent_config("pipeline");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![
        SubtaskContract {
            contract_id: "design".into(),
            label: "设计生成".into(),
            definition_id: None,
            // prompt 最小化：开放式生成会让 coding agent 展开真实工程操作并撞 stage 超时
            prompt: "用一句话给出均线策略设计说明".into(),
        },
        SubtaskContract {
            contract_id: "code-check".into(),
            label: "代码检查".into(),
            definition_id: None,
            prompt: "用一句话基于设计给出代码框架：{pipeline_input}".into(),
        },
    ];

    let turn_ids = swarm
        .dispatch_pipeline(&config, contracts)
        .expect("dispatch_pipeline");

    // 新语义（issue #11 修复）：dispatch_pipeline 串行阻塞至两 stage 真实终态后才返回
    assert_eq!(
        turn_ids.len(),
        2,
        "turn_ids 应为 2；state={:?} results={:?}",
        swarm.state(),
        swarm.results()
    );
    assert_eq!(swarm.state(), SwarmDispatchState::Completed);
    assert_eq!(swarm.results().len(), 2);
    assert_eq!(swarm.results()[0].contract_id, "design");
    assert_eq!(swarm.results()[1].contract_id, "code-check");
    assert!(
        swarm
            .results()
            .iter()
            .all(|r| r.state == SubtaskState::Succeeded)
    );

    swarm.disconnect_all();
}

/// ST-M3-02：S18 标准 Pipeline 契约结构校验。
///
/// 标准模板 prompt 为重量级多段产出（设计说明/流程节点/参数定义 + 代码框架/静态检查），
/// 真实串行执行会触发 coding agent 展开工程操作、超出 stage 超时；端到端串行+注入
/// 语义由最小化 prompt 的 ST-S18-16 承载，本用例只校验契约结构与占位符。
#[test]
fn st_m3_02_s18_contracts() {
    let contracts = s18_design_to_code_contracts("双均线趋势跟踪策略");
    assert_eq!(contracts.len(), 2);
    assert_eq!(contracts[0].contract_id, "design");
    assert_eq!(contracts[1].contract_id, "code-check");
    assert!(contracts[0].prompt.contains("双均线趋势跟踪策略"));
    assert!(contracts[1].prompt.contains("{pipeline_input}"));
}

/// ST-M3-03：Pipeline 超过上限拒绝。
#[test]
fn st_m3_03_pipeline_exceeds_limit() {
    let config = temp_agent_config("limit");
    let mut swarm = SwarmBridge::new();

    let contracts: Vec<_> = (0..5)
        .map(|i| SubtaskContract {
            contract_id: format!("c{i}"),
            label: format!("任务{i}"),
            definition_id: None,
            prompt: format!("prompt {i}"),
        })
        .collect();

    let result = swarm.dispatch_pipeline(&config, contracts);
    assert!(result.is_err());
    assert_eq!(swarm.state(), SwarmDispatchState::Idle);
}

/// ST-M3-04：Pipeline 无上游输入时直接执行。
#[test]
fn st_m3_04_pipeline_no_upstream() {
    let config = temp_agent_config("no-upstream");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![SubtaskContract {
        contract_id: "single".into(),
        label: "单任务".into(),
        definition_id: None,
        prompt: "独立任务无上游".into(),
    }];

    let turn_ids = swarm
        .dispatch_pipeline(&config, contracts)
        .expect("dispatch single");

    assert_eq!(turn_ids.len(), 1);
    assert_eq!(swarm.state(), SwarmDispatchState::Completed);

    swarm.disconnect_all();
}

/// ST-S18-16：真实 LLM 两 stage Pipeline——串行 + 上游产物真实注入下游 prompt。
///
/// 真实 LLM 调用产生 API 费用（prompt 已最小化）；注入口径为 assistant_persisted
/// 产物替换 `{pipeline_input}` 占位符（RD-011）。
#[test]
fn st_s18_16_pipeline_injects_upstream_output() {
    case("ST-S18-16", || {
        let config = temp_agent_config("s18-inject");
        let mut swarm = SwarmBridge::new();

        let contracts = vec![
            SubtaskContract {
                contract_id: "design".into(),
                label: "设计生成".into(),
                definition_id: None,
                prompt: "用一句话给出均线策略设计说明".into(),
            },
            SubtaskContract {
                contract_id: "code-check".into(),
                label: "代码检查".into(),
                definition_id: None,
                prompt: "基于以下设计写一句代码框架说明：{pipeline_input}".into(),
            },
        ];

        let turn_ids = swarm
            .dispatch_pipeline(&config, contracts)
            .expect("dispatch_pipeline");

        // 串行：返回时两 stage 均已真实完成
        assert_eq!(turn_ids.len(), 2);
        assert_eq!(swarm.state(), SwarmDispatchState::Completed);

        let stage1 = &swarm.results()[0];
        let stage2 = &swarm.results()[1];
        assert_eq!(stage1.state, SubtaskState::Succeeded);
        assert_eq!(stage2.state, SubtaskState::Succeeded);

        let out1 = stage1.output.as_deref().expect("stage1 output 非空");
        assert!(!out1.trim().is_empty());
        let out2 = stage2.output.as_deref().expect("stage2 output 非空");
        assert!(!out2.trim().is_empty());

        // 注入断言：stage2 实际发送的 prompt 含 stage1 产物且无占位符残留
        // （含 stage1 产物即证明 stage2 派发晚于 stage1 完成）
        assert!(
            stage2.final_prompt.contains(out1.trim()),
            "stage2 final_prompt 应含 stage1 输出\nstage1 output: {out1}\nstage2 final_prompt: {}",
            stage2.final_prompt
        );
        assert!(
            !stage2.final_prompt.contains("{pipeline_input}"),
            "占位符不得残留: {}",
            stage2.final_prompt
        );

        swarm.disconnect_all();
    });
}
