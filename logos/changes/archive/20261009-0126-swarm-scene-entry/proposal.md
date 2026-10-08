# 变更提案：多 Agent 场景入口接线（swarm-scene-entry）

> module: core | created: 2026-10-08
> 关联 issue：#26 #15（GitHub dorname/nautilus_trader）
> 前置变更：swarm-truthful-dispatch（已归档，终态口径与 Pipeline 注入已修复）

## 变更原因

`swarm-truthful-dispatch` 修复后，多 Agent 底层能力（真实终态、真实注入）已可信，但 `SubtaskResult`/`poll_all`/`dispatch_*` 在 `src/` 应用层**没有任何调用点**（仅 tests 使用），即"能力存在、场景入口缺失"：

1. **#26（P1）确认需求不触发 S17 派发**：GUI `do_confirm_requirement`（app.rs）只写本地需求版本，从不调用 `SwarmBridge` / `s17_requirement_contracts` / `dispatch_parallel`。core-06 M2 设计的"需求确认后并发派发数据可行性/反例检索/约束核对三个分析 sub-agent，聚合结果写回项目状态"在 GUI 旅程中实际不发生，多 Agent 对终端用户不可达。
2. **#15（P2）S19 实验不经真实协调器**：`s19_experiment_contracts` 只是 prompt 文本，没有任何 Coordinator 调用。core-05 硬约束要求"实验数值必须来自协调器计算任务、不得由 LLM 生成"，当前缺失"LLM 整理参数 → 真实协调器执行"的闭环链路。

## 变更类型

代码级变更（含测试规格 delta）：把已验证的多 Agent 能力接到 S17/S19 场景入口，不改动已归档的终态/Pipeline 语义。

## 变更范围

- 影响的需求文档：无（语义沿用 core-05/core-06 既有约束）
- 影响的功能规格：无
- 影响的业务场景：S17（需求确认触发三路分析）、S19（实验参数整理与真实执行）
- 影响的测试规格：`logos/resources/test/core-S17-test-cases.md`、`logos/resources/test/core-S19-test-cases.md`
- 影响的代码：
  - `crates/research-desktop/src/swarm_bridge.rs`（新增 `SwarmWorker` 后台线程封装、`parse_run_params` 纯函数、`s19_organize_params_prompt`）
  - `crates/research-desktop/src/workspace.rs`（`RequirementVersion` 增 `swarm_analysis` 字段、`attach_requirement_analysis` 方法）
  - `crates/research-desktop/src/app.rs`（`do_confirm_requirement` 接线自动派发、`poll_swarm_task` 帧循环、三路状态卡、"AI 整理实验参数"入口与二次确认）
  - `crates/research-desktop/tests/m2_swarm_integration.rs`、`m4_experiment_integration.rs`、`s17_s20_ai.rs`（新增用例，reporter 入账）
- 影响的 API / DB / 编排测试 / smoke：无

## 部署影响

- 是否需要部署：否
- 部署原因：本地桌面应用内部功能接线，无部署产物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true
design_system_mode: generated
design_system_fallback_reason: ""
pages: []
```

说明：GUI 新增"需求确认后三路分析进行中/完成"的状态卡与"AI 整理实验参数"入口，复用现有 tell_card/卡片与 RunForm 控件，不新增页面、不改设计系统。

## 变更概述

**S17 入口（#26）**：`swarm_bridge.rs` 新增 `SwarmWorker`——后台线程持有 `SwarmBridge`，请求 channel 收派发指令、响应 channel 发进度事件（每轮 `poll_all` 快照 + 终态结果），对齐既有 `AgentWorker` 模式保证 GUI 帧循环不阻塞渲染。`do_confirm_requirement` 确认需求成功后自动以 `s17_requirement_contracts` 派发三契约并建立三路状态卡；帧循环 `poll_swarm_task` 非阻塞收事件更新卡片；终态后聚合三路 output 摘要，经 `workspace.attach_requirement_analysis(req_id, summary)` 挂到对应 R 版本（`RequirementVersion` 新增 `swarm_analysis` 字段）并 tell_card 展示。取消语义对齐既有 `cancel_agent_task`：本地放弃、模型侧不伪造中断、过期结果丢弃。

**S19 入口（#15）**：职责切分——LLM 只做意图/参数整理，数值计算只走真实协调器。新增纯函数 `parse_run_params`：从 LLM 回复提取 JSON（容忍 ```json fence），严格校验（非 JSON / 缺字段 / fast>=slow / top_k=0 / 非正金额 / 非法日期一律拒绝），输出只含表单参数、不得含任何结果数值；`build_run_form` 映射到既有 `RunForm`。app 层新增"AI 整理实验参数"入口：AgentWorker turn → parse → 填充运行页表单 → 用户**二次确认**后走现有 `bridge.submit_run`（不自动提交）；实验数值仍只来自 TaskView/equity 并绑定 task_id（沿用现有链路，不新增数值路径）。

测试门槛：ST-S17-21（真实 LLM 经 SwarmWorker 派发三契约最小 prompt，进度事件流→终态 Completed→三 output 非空→摘要 attach 成功）、UT-S17-19（attach_requirement_analysis 判定矩阵）、ST-S19-15（真实 LLM 整理参数→parse→真实协调器合成数据跑通→实验绑定 task_id 且数值来自协调器）、UT-S19-16（parse_run_params 判定矩阵）。全部内嵌 OpenLogos reporter 写入 test-results.jsonl。真实 LLM 调用 prompt 最小化控制成本；并发上限 MAX_PARALLEL_DISPATCH=3 不变。
