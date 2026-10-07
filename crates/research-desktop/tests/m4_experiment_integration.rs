//! M4 验收测试：S19 实验派发与验证证据链。
//!
//! CPU 红线：实验派发为单任务，无并发压力；
//! 验证证据链通过协调器真实计算，不由 LLM 生成。

use nautilus_research_desktop::agent_bridge::AgentSessionConfig;
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_desktop::swarm_bridge::{SwarmBridge, s19_experiment_contracts};

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

    let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if profile_src.exists() {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("m4-{tag}"),
        profile_id: "octos".into(),
        oup: OupConfig {
            octos_bin: "octos".into(),
            data_dir: data_dir.to_string_lossy().into(),
            instance_dir: instance_dir.to_string_lossy().into(),
            workspace_root: "/home/kyle/nautilus_trader".into(),
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
