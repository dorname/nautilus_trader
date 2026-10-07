//! M3 验收测试：Pipeline 拓扑（设计→代码检查）与真实 octos serve 的端到端通信。
//!
//! CPU 红线：Pipeline 串行执行，同一时间只有一个 sub-agent 活跃；
//! 每个阶段最多等待 30 秒。

use nautilus_research_desktop::agent_bridge::AgentSessionConfig;
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_desktop::swarm_bridge::{
    SwarmBridge, SwarmDispatchState, SubtaskContract, s18_design_to_code_contracts,
};

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

    let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if profile_src.exists() {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("m3-{tag}"),
        profile_id: "octos".into(),
        oup: OupConfig {
            octos_bin: "octos".into(),
            data_dir: data_dir.to_string_lossy().into(),
            instance_dir: instance_dir.to_string_lossy().into(),
            workspace_root: "/home/kyle/nautilus_trader".into(),
        },
    }
}

/// ST-M3-01：Pipeline 派发 2 个子任务（设计→代码检查），串行完成。
#[test]
fn st_m3_01_pipeline_dispatch() {
    let config = temp_agent_config("pipeline");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![
        SubtaskContract {
            contract_id: "design".into(),
            label: "设计生成".into(),
            prompt: "生成均线策略设计说明".into(),
        },
        SubtaskContract {
            contract_id: "code-check".into(),
            label: "代码检查".into(),
            prompt: "基于设计生成代码：{pipeline_input}".into(),
        },
    ];

    let turn_ids = swarm
        .dispatch_pipeline(&config, contracts)
        .expect("dispatch_pipeline");

    assert_eq!(turn_ids.len(), 2);
    assert!(matches!(
        swarm.state(),
        SwarmDispatchState::Active {
            dispatched: 2,
            total: 2
        }
    ));
    assert_eq!(swarm.results().len(), 2);
    assert_eq!(swarm.results()[0].contract_id, "design");
    assert_eq!(swarm.results()[1].contract_id, "code-check");

    swarm.disconnect_all();
}

/// ST-M3-02：S18 标准 Pipeline 契约。
#[test]
fn st_m3_02_s18_contracts() {
    let config = temp_agent_config("s18");
    let mut swarm = SwarmBridge::new();

    let contracts = s18_design_to_code_contracts("双均线趋势跟踪策略");
    assert_eq!(contracts.len(), 2);
    assert_eq!(contracts[0].contract_id, "design");
    assert_eq!(contracts[1].contract_id, "code-check");

    let turn_ids = swarm
        .dispatch_pipeline(&config, contracts)
        .expect("dispatch s18 pipeline");

    assert_eq!(turn_ids.len(), 2);
    assert!(matches!(
        swarm.state(),
        SwarmDispatchState::Active {
            dispatched: 2,
            total: 2
        }
    ));

    swarm.disconnect_all();
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
        prompt: "独立任务无上游".into(),
    }];

    let turn_ids = swarm
        .dispatch_pipeline(&config, contracts)
        .expect("dispatch single");

    assert_eq!(turn_ids.len(), 1);
    assert!(matches!(
        swarm.state(),
        SwarmDispatchState::Active {
            dispatched: 1,
            total: 1
        }
    ));

    swarm.disconnect_all();
}
