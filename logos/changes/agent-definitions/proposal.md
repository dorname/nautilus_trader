# 变更提案：agent-definitions

> module: core | created: 2026-10-10

## 变更原因
关联 issue：**#13**（core-06 AgentDefinition / logos/agents 清单未落地）、
**#22**（octos 会话未注册协调器/回测工具，S19 experiment-runner 无法真跑）。

两者同属 **Agent 能力声明**主题：角色清单（谁可以做什么）与工具边界
（能不能调协调器）是同一张能力包的两侧。

现状核实：
- core-06 §4 定义了 manifest schema 与四个角色（requirement-analyst /
  design-generator / code-reviewer / experiment-runner），但仓库无
  `logos/agents/` 目录，派发契约只是硬编码 prompt 字符串（#13）；
- #22 的场景已被 swarm-scene-entry 的 S19 设计规避并验收（LLM 只整理
  参数、桌面侧直接调协调器，ST-S19-15/ST-GUI-02），core-06 的
  「experiment-runner 持协调器工具」设想与现实口径不符，需 ADR 修订；
  octos stdio 侧无 agent/工具注册 API（#16/#20 已文档化能力子集）。

## 变更类型
设计级（core-06 §4/约束 1 ADR 修订 + logos/agents 清单落地）+ 代码级
（派发契约 definition_id 校验）

## 变更范围
- 影响的需求文档：core-06-multi-agent-architecture.md（§4 角色表
  experiment-runner 行、§5 约束 1）
- 影响的功能规格：core-S17 测试用例（新增 UT-S17-27）
- 影响的业务场景：S17 三路分析派发（契约绑定 definition_id）
- 影响的 API：SubtaskContract + definition_id（Option，serde default 兼容）；
  新增 agent_defs 模块（内嵌 manifest 解析/查询/校验）
- 影响的 DB 表：无
- 影响的编排测试：m2_swarm_integration.rs（新 UT）；m2/m3 既有契约字面量
  补 definition_id 字段

## 部署影响
- 是否需要部署：否
- 部署原因：清单经 include_str! 编译期内嵌，无运行时文件依赖
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
1. **落地 logos/agents 清单（#13）**：按 core-06 §4 schema 创建四份
   manifest（requirement-analyst / design-generator / code-reviewer /
   experiment-runner），含 tools/disallowed_tools/model/职责摘要；
   experiment-runner 的 disallowed_tools 显式禁协调器调用（呼应 #22 ADR）。
2. **manifest 内嵌与校验**：`agent_defs` 模块 include_str! 内嵌四份 toml，
   启动可解析；SubtaskContract 增 definition_id（Option），派发时若声明
   则必须在清单中存在，否则拒绝派发（工具白名单的约束力落在校验上）；
   S17 三契约绑定 requirement-analyst。
3. **#22 ADR 修订**：core-06 §4 角色表 experiment-runner 行与 §5 约束 1
   修订为现行口径——实验执行由桌面侧直接调协调器，LLM 只整理参数表单、
   不持协调器工具、不产出数值。
4. 测试：UT-S17-27（manifest 内嵌解析齐全 + definition_id 校验矩阵，
   无 LLM）。
