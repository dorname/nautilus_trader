# 实现任务

## [delta] M1 OUP 客户端
- [x] 产出 `crates/research-desktop/src/oup.rs` — OUP 客户端：JSON-RPC 编解码、子进程管理、session/open、turn/start、task/list、agent/list
- [x] 产出 `crates/research-desktop/tests/m1_oup_integration.rs` — 5 ST：spawn/session_open/task_list/agent_list/invalid_method/concurrent_spawn

## [delta] M2 Swarm Parallel
- [x] 产出 `crates/research-desktop/src/agent_bridge.rs` — Agent 会话管理：connect/disconnect/submit_prompt/poll_tasks/cancel_task
- [x] 产出 `crates/research-desktop/src/swarm_bridge.rs` — Swarm 编排：dispatch_parallel、SubtaskContract、SubtaskResult、S17 三契约
- [x] 产出 `crates/research-desktop/tests/m2_swarm_integration.rs` — 5 ST：dispatch_parallel/s17_contracts/exceeds_limit/cancel_all/poll_all

## [delta] M3 Pipeline
- [x] 产出 `swarm_bridge.rs` 扩展 — dispatch_pipeline：串行拓扑、{pipeline_input} 占位符注入、S18 设计→代码检查契约
- [x] 产出 `crates/research-desktop/tests/m3_pipeline_integration.rs` — 4 ST：pipeline_dispatch/s18_contracts/exceeds_limit/no_upstream

## [delta] M4 S19 Experiment
- [x] 产出 `swarm_bridge.rs` 扩展 — s19_experiment_contracts：版本冻结引用、证据边界声明、协调器集成约束
- [x] 产出 `crates/research-desktop/tests/m4_experiment_integration.rs` — 4 ST：experiment_dispatch/version_freeze/empty_version/evidence_boundary

## [delta] 架构文档
- [x] 产出 `logos/resources/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md` — 多 Agent 生产架构设计

## [delta] 合规化
- [ ] 运行 `openlogos verify` 并生成 test-results.jsonl
- [ ] 用户确认后 `openlogos merge multi-agent-oup-integration`
