//! M2 验收测试：Swarm 并发派发与真实 octos serve 的端到端通信。
//!
//! CPU 红线：每个测试独立临时目录；并发上限 MAX_PARALLEL_DISPATCH=3；
//! 测试完成后清理所有子进程。

#[path = "support/profile.rs"]
mod test_profile;

use nautilus_research_desktop::agent_bridge::{AgentBridge, AgentError, AgentSessionConfig};
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_desktop::swarm_bridge::{
    OutputSource, SwarmBridge, SwarmDispatchState, SwarmError, SwarmWorker, SubtaskContract,
    SubtaskResult, SubtaskState, mark_cancel_outcomes, reduce_dispatch_terminal,
    s17_requirement_contracts, summarize_swarm_results, aggregate_valid,
};
use nautilus_research_desktop::swarm_ledger::{LedgerState, SwarmLedger, recover_on_startup};
use nautilus_research_desktop::workspace::Workspace;
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

    // 复制 profile 配置（issue #24：经 HOME 解析，禁止机器绝对路径硬编码）
    let profile_src = test_profile::configured_source();
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if let Some(src) = profile_src {
        std::fs::copy(src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("m2-{tag}"),
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

/// ST-M2-01：Swarm 并发派发 3 个子任务，全部成功连接。
#[test]
fn st_m2_01_swarm_dispatch_parallel() {
    let config = temp_agent_config("dispatch");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![
        SubtaskContract {
            contract_id: "task-a".into(),
            label: "任务A".into(),
            definition_id: None,
            prompt: "分析数据可行性".into(),
        },
        SubtaskContract {
            contract_id: "task-b".into(),
            label: "任务B".into(),
            definition_id: None,
            prompt: "检索反例".into(),
        },
        SubtaskContract {
            contract_id: "task-c".into(),
            label: "任务C".into(),
            definition_id: None,
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
            definition_id: None,
            prompt: format!("prompt {i}"),
        })
        .collect();

    let result = swarm.dispatch_parallel(&config, contracts);
    assert!(result.is_err());
    assert!(!swarm.is_active());
}

/// ST-S17-22：真实 LLM 派发中 cancel_all（turn/interrupt）——子任务终态
/// Cancelled（非 Succeeded），归约 Failed，无假成功（issue #19/#28/#29）。
#[test]
fn st_s17_22_cancel_all_interrupts_real_turn() {
    case("ST-S17-22", || {
        let config = temp_agent_config("cancel");
        let mut swarm = SwarmBridge::new();

        // 长输出 prompt 保证 turn 在 cancel_all 时仍活跃（interrupt 落在执行中）
        let contracts = vec![
            SubtaskContract {
                contract_id: "cancel-a".into(),
                label: "取消测试A".into(),
                definition_id: None,
                prompt: "从 1 数到 200，每个数字单独一行，不要输出任何其他内容。".into(),
            },
            SubtaskContract {
                contract_id: "cancel-b".into(),
                label: "取消测试B".into(),
                definition_id: None,
                prompt: "从 1 数到 200，每个数字单独一行，不要输出任何其他内容。".into(),
            },
        ];

        let turn_ids = swarm
            .dispatch_parallel(&config, contracts)
            .expect("dispatch");
        assert!(swarm.is_active());
        // issue #29：SubtaskResult 持有 turn_id，cancel 可按 turn 对齐
        for (r, tid) in swarm.results().iter().zip(turn_ids.iter()) {
            assert_eq!(&r.turn_id, tid, "子任务应保留派发返回的 turn_id");
            assert!(!r.turn_id.is_empty());
        }

        // 真实中断：interrupt 真实发出 → 全部 Cancelled 且整体 Ok
        swarm.cancel_all().expect("活跃 turn 应可中断");
        for r in swarm.results() {
            assert_eq!(
                r.state,
                SubtaskState::Cancelled,
                "中断后子任务应为 Cancelled（非 Succeeded）：{}",
                r.contract_id
            );
        }
        assert_eq!(
            swarm.state(),
            SwarmDispatchState::Failed,
            "取消归约必须为 Failed（不得吞为 Completed）"
        );

        swarm.disconnect_all();
    });
}

/// UT-S17-24：OUP 子进程环境（oup-session-config 提案，issue #14）——
/// spawn 级断言（connect 仅启动 octos serve，不调用 LLM）：
/// stderr 落盘日志存在、profile env_vars 注入 extra_env、cwd 为 workspace_root。
#[test]
fn ut_s17_24_child_process_environment() {
    case("UT-S17-24", || {
        let config = temp_agent_config("env");
        let mut bridge = AgentBridge::new(config.clone());
        bridge.connect().expect("connect（spawn octos，不调 LLM）");

        // 1. stderr 落盘（原为 Stdio::null 静默吞掉）
        let log = std::path::Path::new(&config.oup.data_dir).join("logs/octos-stderr.log");
        assert!(log.exists(), "stderr 日志应落盘：{}", log.display());

        // 2. profile env_vars 注入（只断言键名，值不落断言/日志防泄漏）
        let keys = bridge.test_extra_env_keys();
        assert!(
            keys.iter().any(|k| k == "KIMI_API_KEY"),
            "profile env_vars 应注入子进程环境：{keys:?}"
        );

        // 3. 子进程 cwd = workspace_root（经 /proc 核实）
        let pid = bridge.test_child_pid().expect("子进程 PID");
        let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).expect("读子进程 cwd");
        let expect = std::fs::canonicalize(&config.oup.workspace_root).expect("规范化 workspace_root");
        assert_eq!(cwd, expect, "子进程 cwd 应为 workspace_root");

        bridge.disconnect();
    });
}

/// UT-S17-20：取消/中断判定矩阵（无 LLM）——无活跃 turn 拒绝 interrupt；
/// mark_cancel_outcomes 全发出/部分发出/无 Running；无 agent 连接时
/// cancel_all 不得伪装成功（Err + Failed 而非 Cancelled）。
#[test]
fn ut_s17_20_cancel_interrupt_matrix() {
    case("UT-S17-20", || {
        // 1. 无活跃 turn：interrupt_turn 明确拒绝且不改状态
        let mut bridge = AgentBridge::new(AgentSessionConfig::default());
        let before = bridge.state().clone();
        assert!(matches!(
            bridge.interrupt_turn(),
            Err(AgentError::NoActiveTurn)
        ));
        assert_eq!(bridge.state(), &before);

        // 2. mark_cancel_outcomes 纯函数矩阵
        let mk = |id: &str, state: SubtaskState| SubtaskResult {
            contract_id: id.into(),
            state,
            output: None,
            output_source: nautilus_research_desktop::swarm_bridge::OutputSource::Envelope,
            error: None,
            turn_id: format!("t-{id}"),
            final_prompt: String::new(),
            tool_trace: Vec::new(),
        };
        // 全发出 → 全 Cancelled，missed=0
        let mut rs = vec![
            mk("a", SubtaskState::Running),
            mk("b", SubtaskState::Running),
        ];
        assert_eq!(mark_cancel_outcomes(&mut rs, &[true, true]), 0);
        assert!(rs.iter().all(|r| r.state == SubtaskState::Cancelled));
        // 部分发出 → 未发出 Failed + 诚实错误，missed=1；已终态子任务不受影响
        let mut rs = vec![
            mk("a", SubtaskState::Running),
            mk("b", SubtaskState::Running),
            mk("c", SubtaskState::Succeeded),
        ];
        assert_eq!(mark_cancel_outcomes(&mut rs, &[true, false, true]), 1);
        assert_eq!(rs[0].state, SubtaskState::Cancelled);
        assert_eq!(rs[1].state, SubtaskState::Failed);
        assert!(rs[1].error.as_deref().is_some_and(|e| e.contains("未发出")));
        assert_eq!(rs[2].state, SubtaskState::Succeeded);
        // 无 Running → missed=0 且无任何改动
        let mut rs = vec![mk("a", SubtaskState::Succeeded)];
        assert_eq!(mark_cancel_outcomes(&mut rs, &[true]), 0);
        assert_eq!(rs[0].state, SubtaskState::Succeeded);

        // 3. 无 agent 连接（中断发不出）：cancel_all 必须 Err 且标 Failed，
        //    不得伪装 Cancelled/成功（issue #28 核心口径）
        let mut swarm = SwarmBridge::new();
        swarm.test_seed_running("x", "t-x");
        swarm.test_seed_running("y", "t-y");
        assert!(matches!(
            swarm.cancel_all(),
            Err(SwarmError::CancelIncomplete { missed: 2 })
        ));
        for r in swarm.results() {
            assert_eq!(r.state, SubtaskState::Failed);
            assert!(r.error.is_some());
        }
        assert_eq!(swarm.state(), SwarmDispatchState::Failed);
    });
}

/// ST-M2-05：轮询所有子任务状态。
#[test]
fn st_m2_05_poll_all() {
    let config = temp_agent_config("poll");
    let mut swarm = SwarmBridge::new();

    let contracts = vec![SubtaskContract {
        contract_id: "poll-a".into(),
        label: "轮询测试".into(),
        definition_id: None,
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

        let mut contracts = s17_requirement_contracts(
            "研究日线双均线策略在震荡市的表现：已有 OHLCV；使用三个固定合成标的；收盘形成信号、下一交易日开盘执行；投入上限100%，佣金万三，现金不足拒绝订单。",
        );
        for contract in &mut contracts {
            contract.prompt.push_str("\n这是非交互固定样本分析，所有前提已给定。直接用一到两句话返回你的分析；不要向用户提问，也不要调用任何工具。未知量列为限制即可。");
        }
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
        // 拥塞期实测尾延迟 270s+：预算放宽到 480s（断言口径不变，只放宽等待）
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(480);
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

/// ST-S17-21：真实 LLM 经 SwarmWorker 派发三契约——进度事件流（至少一次非终态
/// Active）→ 终态 Completed → 三 output 非空 → 聚合摘要 attach 到需求版本可读回。
///
/// 真实 LLM 调用产生 API 费用（prompt 已最小化）；worker 事件泵间隔 500ms，
/// 总超时 300s（吸收 moonshot 尾延迟）。
#[test]
fn st_s17_21_swarm_worker_progress_to_attach() {
    case("ST-S17-21", || {
        let config = temp_agent_config("s17-worker");
        let worker = SwarmWorker::spawn(config);

        // 最小 prompt 三契约：验证 worker 机制（契约模板本身由 ST-S17-19 覆盖）
        let contracts = (0..3)
            .map(|i| SubtaskContract {
                contract_id: format!("w-{i}"),
                label: format!("分析{i}"),
                definition_id: None,
                prompt: "用一句话回答：1+1等于几？".into(),
            })
            .collect();
        worker.dispatch(1, contracts).expect("dispatch");

        let mut saw_progress = false;
        let mut terminal_event = None;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(320);
        while terminal_event.is_none() {
            assert!(
                std::time::Instant::now() < deadline,
                "320s 内未收到终态事件"
            );
            while let Some(ev) = worker.try_recv() {
                if ev.terminal {
                    terminal_event = Some(ev);
                } else {
                    saw_progress = true;
                    assert!(
                        matches!(ev.state, SwarmDispatchState::Active { .. }),
                        "非终态事件应为 Active，实际：{:?}",
                        ev.state
                    );
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(saw_progress, "应收到至少一次 Active 进度事件（不得只有终态）");

        let ev = terminal_event.expect("终态事件");
        assert_eq!(
            ev.state,
            SwarmDispatchState::Completed,
            "终态应为 Completed（error: {:?}）",
            ev.error
        );
        assert_eq!(ev.results.len(), 3);
        for r in &ev.results {
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

        // 聚合摘要 attach 到需求版本并可读回（issue #26 挂载口径）
        let mut w = Workspace::new();
        let req_id = w
            .confirm_requirement("测试需求", "验收标准", 50.0, 100.0)
            .expect("确认需求");
        let summary = summarize_swarm_results(&ev.results);
        assert!(!summary.trim().is_empty(), "摘要应非空");
        w.attach_requirement_analysis(req_id, &summary)
            .expect("attach 摘要");
        let stored = &w.current().reqs[req_id - 1].swarm_analysis;
        assert_eq!(stored.as_deref(), Some(summary.as_str()), "R_n 应可读回摘要");
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

/// UT-S17-25：swarm 账本（swarm-ledger-adr 提案，issue #23）——写入派发、
/// 终态更新、模拟重启（重开账本）后未完成派发全部标记中断且不残留 Active。
#[test]
fn ut_s17_25_swarm_ledger_recovery() {
    case("UT-S17-25", || {
        let dir = std::env::temp_dir().join(format!(
            "rd-ledger-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let path = dir.join("swarm-ledger.redb");

        // 写入：两笔派发，一笔到终态，一笔保持 Active
        {
            let mut ledger = SwarmLedger::open(&path).expect("open ledger");
            ledger
                .record_dispatch(
                    "d-done",
                    vec!["feasibility".into()],
                    vec!["t-1".into()],
                )
                .expect("record d-done");
            ledger
                .record_terminal("d-done", LedgerState::Completed)
                .expect("terminal d-done");
            ledger
                .record_dispatch(
                    "d-open",
                    vec!["counterexample".into(), "constraint".into()],
                    vec!["t-2".into(), "t-3".into()],
                )
                .expect("record d-open");

            let rec = ledger.get("d-done").expect("get").expect("存在");
            assert_eq!(rec.state, LedgerState::Completed);
            assert_eq!(rec.turn_ids, vec!["t-1".to_string()]);
            let rec = ledger.get("d-open").expect("get").expect("存在");
            assert_eq!(rec.state, LedgerState::Active);
            assert_eq!(rec.contracts.len(), 2);
        } // drop 关闭（模拟进程结束）

        // 模拟重启：未完成派发全部标记中断，不残留 Active
        {
            let mut ledger = SwarmLedger::open(&path).expect("reopen ledger");
            let open = ledger.unfinished().expect("unfinished");
            assert_eq!(open.len(), 1);
            assert_eq!(open[0].dispatch_id, "d-open");
            let n = ledger.mark_unfinished_interrupted().expect("mark");
            assert_eq!(n, 1);
            assert!(ledger.unfinished().expect("unfinished").is_empty());
            let rec = ledger.get("d-open").expect("get").expect("存在");
            assert_eq!(rec.state, LedgerState::Interrupted);
            // 已 Completed 的不受影响
            let rec = ledger.get("d-done").expect("get").expect("存在");
            assert_eq!(rec.state, LedgerState::Completed);
        }

        // recover_on_startup 幂等：第二次调用 0 条；不存在路径返回 0
        assert_eq!(recover_on_startup(&path), 0);
        assert_eq!(
            recover_on_startup(&dir.join("nonexistent").join("x.redb")),
            0
        );
    });
}

/// UT-S17-27：Agent 定义清单（agent-definitions 提案，issue #13/#22）——
/// 内嵌 manifest 四定义齐全；场景契约 definition_id 均命中；experiment-runner
/// 禁协调器调用；未知 definition_id 派发被拒绝且无副作用。
#[test]
fn ut_s17_27_agent_definitions_manifest() {
    case("UT-S17-27", || {
        use nautilus_research_desktop::agent_defs::{agent_definition, agent_definitions};

        // 1. 四定义齐全且字段解析成功
        let defs = agent_definitions();
        assert_eq!(defs.len(), 4, "内嵌 manifest 应恰为四份");
        for name in [
            "requirement-analyst",
            "design-generator",
            "code-reviewer",
            "experiment-runner",
        ] {
            let d = agent_definition(name).unwrap_or_else(|| panic!("缺定义 {name}"));
            assert_eq!(d.version, 1);
            assert!(!d.summary.is_empty());
        }

        // 2. experiment-runner 显式禁止协调器调用（issue #22 ADR）
        let er = agent_definition("experiment-runner").expect("experiment-runner");
        assert!(
            er.disallowed_tools
                .iter()
                .any(|t| t == "submit_run" || t == "coordinator"),
            "experiment-runner 必须禁协调器调用：{:?}",
            er.disallowed_tools
        );

        // 3. 场景契约 definition_id 均命中清单
        for c in s17_requirement_contracts("测试需求") {
            let id = c.definition_id.as_deref().expect("S17 契约应声明 definition_id");
            assert!(agent_definition(id).is_some(), "未知 definition_id：{id}");
        }
        for c in nautilus_research_desktop::swarm_bridge::s18_design_to_code_contracts("测试需求")
        {
            let id = c.definition_id.as_deref().expect("S18 契约应声明 definition_id");
            assert!(agent_definition(id).is_some(), "未知 definition_id：{id}");
        }
        for c in
            nautilus_research_desktop::swarm_bridge::s19_experiment_contracts("v1", "snap")
        {
            let id = c.definition_id.as_deref().expect("S19 契约应声明 definition_id");
            assert!(agent_definition(id).is_some(), "未知 definition_id：{id}");
        }

        // 4. 未知 definition_id：派发被拒绝且无副作用（不建 agent、不改状态）
        let config = temp_agent_config("unknown-def");
        let mut swarm = SwarmBridge::new();
        let bad = vec![SubtaskContract {
            contract_id: "bad".into(),
            label: "未知定义".into(),
            definition_id: Some("no-such-agent".into()),
            prompt: "p".into(),
        }];
        let result = swarm.dispatch_parallel(&config, bad);
        assert!(matches!(
            result,
            Err(SwarmError::UnknownAgentDefinition { .. })
        ));
        assert!(!swarm.is_active(), "拒绝派发后不得进入 Active");
        assert!(swarm.results().is_empty(), "拒绝派发不得产生子任务");
    });
}

/// UT-S17-26：聚合校验判定矩阵（issue #23 M4.3 validator 语义）——
/// 仅全 Succeeded 且 output 非空放行，其余一律阻断。
#[test]
fn ut_s17_26_aggregate_valid_matrix() {
    case("UT-S17-26", || {
        let mk = |state: SubtaskState, output: Option<&str>| SubtaskResult {
            contract_id: "c".into(),
            state,
            output: output.map(str::to_string),
            output_source: nautilus_research_desktop::swarm_bridge::OutputSource::Envelope,
            error: None,
            turn_id: "t".into(),
            final_prompt: String::new(),
            tool_trace: Vec::new(),
        };
        // 全 Succeeded + 非空 output → 放行
        assert!(aggregate_valid(&[
            mk(SubtaskState::Succeeded, Some("ok")),
            mk(SubtaskState::Succeeded, Some("ok2")),
        ]));
        // 空结果集 → 阻断
        assert!(!aggregate_valid(&[]));
        // 含 Failed → 阻断
        assert!(!aggregate_valid(&[
            mk(SubtaskState::Succeeded, Some("ok")),
            mk(SubtaskState::Failed, None),
        ]));
        // 含 Cancelled → 阻断
        assert!(!aggregate_valid(&[mk(SubtaskState::Cancelled, None)]));
        // output 空白串 → 阻断
        assert!(!aggregate_valid(&[mk(SubtaskState::Succeeded, Some("  "))]));
        // output None → 阻断
        assert!(!aggregate_valid(&[mk(SubtaskState::Succeeded, None)]));
    });
}

/// ST-S17-23（embed-octos-runtime）：产物文件型契约 + 托管工作区回流——
/// 三路真实 LLM（允许工具调用，默认 Workspace-Write 免审批档），终态后
/// 至少一路从托管工作区回流文件产物（output_source=ManagedFile 且内容
/// 与工作区文件一致）；envelope 降级路如实标注。真实 LLM 调用产生费用
/// （prompt 已最小化）。
#[test]
fn st_s17_23_managed_workspace_file_roundtrip() {
    case("ST-S17-23", || {
        let config = temp_agent_config("s17-roundtrip");
        let mut swarm = SwarmBridge::new();

        // 不追加「禁用工具」——本用例验证的正是工具写文件与回流。
        let contracts = s17_requirement_contracts(
            "研究日线动量策略的数据可行性与约束（固定合成标的，OHLCV 已给定）。",
        );
        let turn_ids = swarm
            .dispatch_parallel(&config, contracts)
            .expect("dispatch_parallel");
        assert_eq!(turn_ids.len(), 3);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        while swarm.is_active() {
            assert!(std::time::Instant::now() < deadline, "600s 内未达终态");
            swarm.poll_all().expect("poll_all");
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert_eq!(swarm.state(), SwarmDispatchState::Completed, "三路应全部成功");

        let mut file_backed = 0;
        for (i, r) in swarm.results().iter().enumerate() {
            assert_eq!(r.state, SubtaskState::Succeeded, "{}（{:?}）", r.contract_id, r.error);
            assert!(r.output.as_deref().is_some_and(|s| !s.trim().is_empty()));
            if r.output_source == OutputSource::ManagedFile {
                file_backed += 1;
                let ws = swarm.agent_workspaces()[i].join(format!("{}-analysis.md", r.contract_id));
                let disk = std::fs::read_to_string(&ws)
                    .unwrap_or_default();
                assert!(
                    !disk.trim().is_empty(),
                    "{} 工作区产物文件应为非空：{}",
                    r.contract_id,
                    ws.display()
                );
                assert_eq!(
                    r.output.as_deref().map(str::trim),
                    Some(disk.trim()),
                    "{} 回流内容应与工作区文件一致",
                    r.contract_id
                );
            }
        }
        assert!(
            file_backed >= 1,
            "至少一路应从托管工作区回流文件产物（实测 {file_backed}/3；             全 envelope 降级说明工具写文件未生效，需人工核查权限档）"
        );

        swarm.disconnect_all();
    });
}
