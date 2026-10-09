# 变更提案：turn-lifecycle

> module: core | created: 2026-10-09

## 变更原因
关联 issue：**#29**（SwarmBridge 丢弃 turn_id，无法按 turn 做 interrupt/终态对齐）、
**#19**（AgentBridge 取消走 task/cancel，无法中断真实 turn）、**#28**
（cancel_all 在 tasks 为空时本地标 Cancelled，真实 turn 仍 active）。

三者同属 **turn 生命周期**主题，必须一起修才闭环：没有 turn_id 保留就无法
interrupt（#29 是 #19/#28 前置）；只封装 interrupt 而不改 cancel_all 语义，
「已取消但模型继续烧 token」的伪装成功仍在（#28）。

现状（swarm-truthful-dispatch 之后）：`SubtaskResult.turn_id` 与
`AgentBridge.active_turn_id` 已存在，终态判定已以真实 turn 事件为准；
但取消路径仍是空白——全 crate `interrupt` 零命中，`cancel_task` 走
`task/cancel`（对活跃 turn 无效），`cancel_all` 把 Running 一律本地改
Cancelled 且恒 `Ok(())`；GUI 任务条「取消」是纯本地放弃语义。

## 变更类型
接口级 + 代码级（OupClient/AgentBridge/SwarmBridge 公共 API 新增 interrupt
通路；取消语义修正为诚实口径，不伪造中断成功）

## 变更范围
- 影响的需求文档：无（core-06「取消 / 重试任务」口径的实现落地）
- 影响的功能规格：core-S17 测试用例（新增 UT-S17-20 / ST-S17-22）
- 影响的业务场景：S17 多 Agent 派发取消、S19/对话 agent 任务取消（GUI 任务条）
- 影响的 API：OupClient + turn_interrupt；AgentBridge + interrupt_turn；
  SwarmBridge::cancel_all 语义重写（interrupt 发出才标 Cancelled，未发出标
  Failed 且整体返回 Err）；GUI 任务条「取消」升级为真实 interrupt + 本地放弃
- 影响的 DB 表：无
- 影响的编排测试：m2_swarm_integration.rs（st_m2_04 重写为 reporter 入账新口径）

## 部署影响
- 是否需要部署：否
- 部署原因：库与桌面行为修正，无独立部署物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false            # 本次是否触及界面（GUI 项目才有意义）
design_system_mode: generated   # generated | fallback（fallback 时须填 design_system_fallback_reason）
design_system_fallback_reason: ""
pages: []                   # 每项 {id, prototype: core-NN-<slug>.html, description}
```

## 变更概述
1. `OupClient::turn_interrupt(session_id, turn_id)`：封装 `turn/interrupt` RPC
   （真实探针已验证 `{interrupted:true}` → `turn/error(code=interrupted)`）。
2. `AgentBridge::interrupt_turn()`：按 `active_turn_id` 发真实中断；无活跃
   turn 明确拒绝；状态迁移区分 interrupted（取消成功）与 failed（错误），
   UI 可辨别。
3. `SwarmBridge::cancel_all` 重写：逐个子任务按其 `turn_id` 对所属 agent 发
   `turn/interrupt`；**只有中断请求真实发出的子任务才标 Cancelled**；发不出
   的标 Failed 并汇总错误，整体返回 `Err`（空 tasks 不再伪装成功取消）。
4. `AgentWorker`/`SwarmWorker`/GUI 任务条「取消」接线：先真实 interrupt，
   再本地放弃等待（不伪造已完成中断以外的语义）；worker 超时路径同步改用
   interrupt 语义。
5. 测试：UT-S17-20（无 LLM 状态迁移/归约矩阵）、ST-S17-22（真实 LLM 长
   prompt 派发 → interrupt → `turn/error` interrupted 终态，无假成功）、
   st_m2_04 重写并入 reporter 新口径。
