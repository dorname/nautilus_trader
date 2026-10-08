# 变更提案：Swarm 派发真实性修复（swarm-truthful-dispatch）

> module: core | created: 2026-10-08
> 关联 issue：#10 #11 #12（GitHub dorname/nautilus_trader，2026-10-08 动态复现）
> 前置变更：multi-agent-oup-integration（已归档）、wire-real-llm-oup（已归档）

## 变更原因

2026-10-08 真实 LLM 多进程探针在 main @ 8a4835a69d 上动态复现三个 P0 缺陷：

1. **#12 终态口径错误（假成功）**：`SwarmBridge::poll_all` 以 `task/list` 为空判定"全部终态"，真实派发后 3 秒即置 `Completed` 并把 Running 子任务全部标 `Succeeded` 且 `output=None`（真实 LLM 此刻仍在运行，单 turn 基线约 91s）；即使 task 为 failed/cancelled 也被吞为成功。违反 core-05"未经校验/未运行的产物不得标通过"硬约束。
2. **#11 Pipeline 语义无效**：`dispatch_pipeline` 的 `previous_output` 恒为空 String，两 stage 的 turn 同时起跑，`{pipeline_input}` 占位符以字面量发给下游 LLM，上游输出从未注入。
3. **#10 验收口径缺口**：M1-M4 集成测试只验证派发（Running 状态）不等待真实终态，导致 #11/#12 全部漏网；且 ST-M* 用例 ID 未进入 `logos/resources/test/` 验收口径，测试也未内嵌 OpenLogos reporter。

## 变更类型

代码级修复（含测试规格 delta）：修复 swarm 终态判定与 Pipeline 注入语义，收紧 M2/M3/M4 验收门槛到真实终态 + 非空产物，并将用例纳入验收 spec。

## 变更范围

- 影响的需求文档：无（语义沿用 core-05/core-06 既有约束）
- 影响的功能规格：无
- 影响的业务场景：S17（需求多 Agent 协作）、S18（设计→代码检查 Pipeline）、S19（实验派发口径沿用）
- 影响的测试规格：`logos/resources/test/core-S17-test-cases.md`、`logos/resources/test/core-S18-test-cases.md`
- 影响的代码：
  - `crates/research-desktop/src/swarm_bridge.rs`（poll_all 终态口径、dispatch_pipeline 串行注入、SubtaskResult 增 turn_id/final_prompt）
  - `crates/research-desktop/src/agent_bridge.rs`（新增非阻塞 turn 终态轮询）
  - `crates/research-desktop/tests/m2_swarm_integration.rs`、`m3_pipeline_integration.rs`、`m4_experiment_integration.rs`（等终态 + 断言产物 + reporter）
- 影响的 API / DB / 编排测试 / smoke：无

## 部署影响

- 是否需要部署：否
- 部署原因：本地桌面应用内部库修复，无部署产物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false
design_system_mode: generated
design_system_fallback_reason: ""
pages: []
```

## 变更概述

终态口径从 task/list 改为 turn 事件流：`SubtaskResult` 记录 `turn_id`，`AgentBridge` 新增非阻塞 `poll_turn_outcome`（排空 notification channel，匹配本 turn 的 `turn/completed` / `turn/error`），`poll_all` 据此把子任务置为 Succeeded（填 `output=assistant_text`）或 Failed（填 `error`）；全部终态时有失败则总体 Failed，空 task/list 不再参与判定。

Pipeline 改为真串行：`dispatch_pipeline` 每 stage 使用 `submit_and_wait` 阻塞等待上游 `turn/completed`，将上游 `assistant_text` 注入下游 prompt 的 `{pipeline_input}`；任一 stage 失败立即终止，未派发 stage 标 Cancelled，总体 Failed。`SubtaskResult` 增 `final_prompt` 记录实际发送的 prompt，供测试断言注入真实发生。

测试门槛收紧：M2/M3/M4 派发后必须带超时轮询至真实终态，断言全部 Succeeded 且 `output` 非空；新增"首轮轮询不得假成功"反例用例与 Pipeline 注入断言（下游 `final_prompt` 含上游输出、不含占位符字面量）。所有改造测试内嵌 OpenLogos reporter，用例 ID 经 delta 写入验收 spec（ST-S17-19/ST-S17-20/UT-S17-18/ST-S18-16）。

CPU 红线不变：并发上限 MAX_PARALLEL_DISPATCH=3，每子 agent 独立数据目录；真实 LLM 调用 prompt 最小化控制成本。
