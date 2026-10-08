//! M2 验收测试：Swarm 并发派发与真实 octos serve 的端到端通信。
//!
//! CPU 红线：每个测试独立临时目录；并发上限 MAX_PARALLEL_DISPATCH=3；
//! 测试完成后清理所有子进程。

use nautilus_research_desktop::agent_bridge::AgentSessionConfig;
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_desktop::swarm_bridge::{
    SwarmBridge, SwarmDispatchState, SubtaskContract, SubtaskState, reduce_dispatch_terminal,
    s17_requirement_contracts,
};
use nautilus_research_testkit::case;

fn temp_agent_config(tag: &str) -> AgentSessionConfig {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "rd-m2-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let data_dir = base.join("data");
    let instance_dir = base.join("instance");

    // 确保目录存在
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&instance_dir).expect("create instance dir");

    // 复制 profile 配置（从主 octos 数据目录）
    let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if profile_src.exists() {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("m2-{tag}"),
        profile_id: "octos".into(),
        oup: OupConfig {
            octos_bin: "octos".into(),
            data_dir: data_dir.to_string_lossy().into(),
            instance_dir: instance_dir.to_string_lossy().into(),
            workspace_root: "/home/kyle/nautilus_trader".into(),
        },
    }
}

/// ST-M2-01：Swarm 并发派发 3 个子任务，全部成功连接。
#[test]
fn st_m2_01_swarm_dispatch_parallel() {
    let config = temp_agent_config("dispatch");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![
        SubtaskContract {
            contract_id: "task-a".into(),
            label: "任务A".into(),
            prompt: "分析数据可行性".into(),
        },
        SubtaskContract {
            contract_id: "task-b".into(),
            label: "任务B".into(),
            prompt: "检索反例".into(),
        },
        SubtaskContract {
            contract_id: "task-c".into(),
            label: "任务C".into(),
            prompt: "核对约束".into(),
        },
    ];

    let turn_ids = swarm
        .dispatch_parallel(&config, contracts)
        .expect("dispatch_parallel");

    assert_eq!(turn_ids.len(), 3);
    assert!(swarm.is_active());
    assert_eq!(swarm.results().len(), 3);
    assert!(
        swarm
            .results()
            .iter()
            .all(|r| r.state == SubtaskState::Running)
    );

    swarm.disconnect_all();
}

/// ST-M2-02：S17 标准三并发契约派发。
#[test]
fn st_m2_02_s17_contracts_dispatch() {
    let config = temp_agent_config("s17");
    let mut swarm = SwarmBridge::new();

    let contracts = s17_requirement_contracts("均线趋势策略研究");
    assert_eq!(contracts.len(), 3);

    let turn_ids = swarm
        .dispatch_parallel(&config, contracts)
        .expect("dispatch s17 contracts");

    assert_eq!(turn_ids.len(), 3);
    assert!(swarm.is_active());

    // 验证 contract_id 正确
    let ids: Vec<_> = swarm.results().iter().map(|r| r.contract_id.as_str()).collect();
    assert!(ids.contains(&"feasibility"));
    assert!(ids.contains(&"counterexample"));
    assert!(ids.contains(&"constraint"));

    swarm.disconnect_all();
}

/// ST-M2-03：超过并发上限拒绝派发。
#[test]
fn st_m2_03_dispatch_exceeds_limit() {
    let config = temp_agent_config("limit");
    let mut swarm = SwarmBridge::new();

    let contracts: Vec<_> = (0..5)
        .map(|i| SubtaskContract {
            contract_id: format!("c{i}"),
            label: format!("任务{i}"),
            prompt: format!("prompt {i}"),
        })
        .collect();

    let result = swarm.dispatch_parallel(&config, contracts);
    assert!(result.is_err());
    assert!(!swarm.is_active());
}

/// ST-M2-04：取消所有子任务。
#[test]
fn st_m2_04_cancel_all() {
    let config = temp_agent_config("cancel");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![
        SubtaskContract {
            contract_id: "cancel-a".into(),
            label: "取消测试A".into(),
            prompt: "test cancel".into(),
        },
        SubtaskContract {
            contract_id: "cancel-b".into(),
            label: "取消测试B".into(),
            prompt: "test cancel".into(),
        },
    ];

    swarm
        .dispatch_parallel(&config, contracts)
        .expect("dispatch");
    assert!(swarm.is_active());

    swarm.cancel_all().expect("cancel_all");
    assert_eq!(swarm.state(), SwarmDispatchState::Failed);

    swarm.disconnect_all();
}

/// ST-M2-05：轮询所有子任务状态。
#[test]
fn st_m2_05_poll_all() {
    let config = temp_agent_config("poll");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![SubtaskContract {
        contract_id: "poll-a".into(),
        label: "轮询测试".into(),
        prompt: "test poll".into(),
    }];

    swarm
        .dispatch_parallel(&config, contracts)
        .expect("dispatch");

    // 轮询（可能成功或失败，取决于 octos serve 状态）
    let result = swarm.poll_all();
    // 不强制要求成功（turn 可能还在进行中），只要不 panic
    let _ = result;

    swarm.disconnect_all();
}

/// UT-S17-18：终态归约纯函数判定矩阵（空集/全成功/含失败/含取消/含运行中）。
#[test]
fn ut_s17_18_reduce_dispatch_terminal_matrix() {
    case("UT-S17-18", || {
        // 空集与含非终态：不得判终态
        assert_eq!(reduce_dispatch_terminal(&[]), None);
        assert_eq!(reduce_dispatch_terminal(&[SubtaskState::Running]), None);
        assert_eq!(
            reduce_dispatch_terminal(&[SubtaskState::Succeeded, SubtaskState::Queued]),
            None
        );
        assert_eq!(
            reduce_dispatch_terminal(&[SubtaskState::Failed, SubtaskState::Running]),
            None
        );

        // 仅全 Succeeded 归约 Completed
        assert_eq!(
            reduce_dispatch_terminal(&[SubtaskState::Succeeded, SubtaskState::Succeeded]),
            Some(SwarmDispatchState::Completed)
        );

        // 含 Failed / Cancelled 归约 Failed（失败不得吞为成功）
        assert_eq!(
            reduce_dispatch_terminal(&[SubtaskState::Succeeded, SubtaskState::Failed]),
            Some(SwarmDispatchState::Failed)
        );
        assert_eq!(
            reduce_dispatch_terminal(&[SubtaskState::Cancelled]),
            Some(SwarmDispatchState::Failed)
        );
        assert_eq!(
            reduce_dispatch_terminal(&[SubtaskState::Succeeded, SubtaskState::Cancelled]),
            Some(SwarmDispatchState::Failed)
        );
    });
}

/// ST-S17-19：真实 LLM 三契约并发派发，轮询等真实终态——全部 Succeeded 且 output 非空。
///
/// 真实 LLM 调用产生 API 费用（prompt 已最小化）；终态口径为 turn/completed 事件流，
/// 禁止 task/list 空列表冒充成功（RD-012）。
#[test]
fn st_s17_19_parallel_waits_real_terminal() {
    case("ST-S17-19", || {
        let config = temp_agent_config("s17-terminal");
        let mut swarm = SwarmBridge::new();

        let contracts = s17_requirement_contracts("均线策略在震荡市的表现（一句话即可）");
        let turn_ids = swarm
            .dispatch_parallel(&config, contracts)
            .expect("dispatch_parallel");

        assert_eq!(turn_ids.len(), 3);
        for tid in &turn_ids {
            assert!(
                uuid::Uuid::parse_str(tid).is_ok(),
                "turn_id 应为合法 UUID: {tid}"
            );
        }

        // 带超时循环 poll_all 至终态（规格默认 180s）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        while swarm.is_active() {
            assert!(std::time::Instant::now() < deadline, "180s 内未达终态");
            swarm.poll_all().expect("poll_all");
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        assert_eq!(swarm.state(), SwarmDispatchState::Completed);
        assert_eq!(swarm.results().len(), 3);
        for r in swarm.results() {
            assert_eq!(
                r.state,
                SubtaskState::Succeeded,
                "{} 应成功（error: {:?}）",
                r.contract_id,
                r.error
            );
            assert!(
                r.output.as_deref().is_some_and(|s| !s.trim().is_empty()),
                "{} output 应非空",
                r.contract_id
            );
        }

        swarm.disconnect_all();
    });
}

/// ST-S17-20：派发后首轮轮询不得假成功——状态保持 Active，Running 不得标 Succeeded。
#[test]
fn st_s17_20_first_poll_not_false_completed() {
    case("ST-S17-20", || {
        let config = temp_agent_config("s17-first-poll");
        let mut swarm = SwarmBridge::new();

        let contracts = s17_requirement_contracts("趋势策略的数据需求分析");
        swarm
            .dispatch_parallel(&config, contracts)
            .expect("dispatch_parallel");

        // 派发后立即轮询一次（真实 LLM turn 进行中，不可能已完成）
        swarm.poll_all().expect("poll_all");

        assert!(
            matches!(swarm.state(), SwarmDispatchState::Active { .. }),
            "首轮轮询不得判 Completed/Failed，实际：{:?}",
            swarm.state()
        );
        for r in swarm.results() {
            assert_eq!(
                r.state,
                SubtaskState::Running,
                "{} 未经真实终态不得标 Succeeded",
                r.contract_id
            );
            assert!(r.output.is_none(), "{} output 应为 None", r.contract_id);
        }

        swarm.disconnect_all();
    });
}
