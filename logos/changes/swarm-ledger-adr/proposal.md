# 变更提案：swarm-ledger-adr

> module: core | created: 2026-10-09

## 变更原因
关联 issue：**#25**（SwarmBridge 未复用 octos-swarm，自研多进程伪编排）、
**#23**（core-06 聚合校验与 redb 崩溃恢复未在 research-desktop 落地）。

实测结论（2026-10-09）：`octos-swarm` **不作为库公开发布**（crates.io 无此
crate；本机 octos 2.0.3-rc.9 仅以 CLI 分发），research-desktop 作为 OUP
stdio 客户端无法复用其 `Swarm::dispatch`。#25 Expected 允许替代路径：
「明确 ADR 修订为客户端侧并行 turn，并下调 core-06 承诺」——前提条件是
客户端编排补齐终态/取消/持久化。终态（swarm-truthful-dispatch）与取消
（turn-lifecycle 真实 interrupt）已补齐，**剩余缺口即本变更的 redb 账本
持久化（#23）**。

#23 现状核实：聚合校验事实上已存在（poll_swarm_task 的 attach 前置：
全 Succeeded 且 output 非空才挂载），但内联未显式化；redb 账本与
「GUI 重启后重建任务卡片状态」完全缺失。

## 变更类型
设计级（core-06 ADR 修订）+ 代码级（redb 账本与崩溃恢复）

## 变更范围
- 影响的需求文档：core-06-multi-agent-architecture.md（决策表「多 Agent
  编排」「状态持久化」两行 + 约束第 5 条的实现主体修订）
- 影响的功能规格：core-S17 测试用例（新增 UT-S17-25 / UT-S17-26）
- 影响的业务场景：S17 多 Agent 派发（账本落盘、重启重建）
- 影响的 API：新增 swarm_ledger 模块（SwarmLedger）；SwarmWorker dispatch/
  终态写账本；ResearchApp attach_bridge 后读账本重建遗留派发状态；
  聚合校验提取为 swarm_bridge::aggregate_valid 纯函数
- 影响的 DB 表：新增 redb 文件 `{agent_data_dir}/swarm/swarm-ledger.redb`
  （非 SQLite schema 变更）
- 影响的编排测试：m2_swarm_integration.rs（新增 UT 用例）

## 部署影响
- 是否需要部署：否
- 部署原因：库与桌面行为修正，无独立部署物
- 影响环境：无
- 是否涉及数据迁移：否（账本为新文件，首次运行自动创建）
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
1. **ADR 修订（#25）**：core-06 决策表「多 Agent 编排」改为「客户端侧并行
   turn（SwarmBridge 经 OUP stdio）」，理由写明 octos-swarm 不作为库公开
   发布、客户端编排已具备终态归约/真实 interrupt/redb 账本/聚合校验；
   「状态持久化」行的「octos 自带 redb ledger」改为「research-desktop 自建
   redb 账本」。
2. **redb 账本（#23）**：新模块 swarm_ledger：dispatch 写入（dispatch_id、
   契约指纹、turn_id、状态），终态更新；账本打开失败降级不阻断派发
   （记录日志）。SwarmWorker dispatch/终态接线。
3. **崩溃恢复重建（#23 约束 5）**：GUI attach_bridge 后读账本，未完成派发
   一律标记「中断（进程重启）」——诚实口径：octos 子进程已随旧进程死亡，
   不得假装仍在运行；重建为任务记录（record/tell）而非复活任务条。
4. **聚合校验显式化**：attach 前置校验提取为 `aggregate_valid` 纯函数
   （全 Succeeded 且 output 非空），失败阻断产物挂载（既有语义不变）。
5. 测试：UT-S17-25（账本写/读/重启标记矩阵，无 LLM）、UT-S17-26
   （aggregate_valid 判定矩阵，无 LLM）。
