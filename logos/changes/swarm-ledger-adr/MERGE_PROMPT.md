# 合并指令

## 变更提案
- 提案名称：swarm-ledger-adr
- 提案目录：logos/changes/swarm-ledger-adr/

## 提案内容

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


## 需要合并的 Delta 文件

### 1. deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md

- Delta 文件：`logos/changes/swarm-ledger-adr/deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md`
- 目标目录：`logos/resources/prd/3-technical-plan/1-architecture/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/test/core-S17-test-cases.md

- Delta 文件：`logos/changes/swarm-ledger-adr/deltas/test/core-S17-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(swarm-ledger-adr): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive swarm-ledger-adr`。
