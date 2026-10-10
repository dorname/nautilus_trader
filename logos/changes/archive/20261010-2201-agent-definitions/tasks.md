# 实现任务

## [delta] 规格变更
- [x] deltas/prd：core-06 §4 角色表 experiment-runner 行 + §5 约束 1 ADR 修订（#22）
- [x] deltas/test/core-S17-test-cases.md：注册 UT-S17-27（manifest 与 definition_id 校验）

## [code] 代码实现

- [x] logos/agents/ 四份 manifest（requirement-analyst / design-generator / code-reviewer / experiment-runner）
- [x] agent_defs 模块：include_str! 内嵌解析 + 查询（OnceLock）
- [x] SubtaskContract + definition_id；派发校验未知 id 拒绝（UnknownAgentDefinition）；S17/S18/S19 契约绑定对应定义
- [x] m2/m3 既有契约字面量补 definition_id 字段
- [x] UT-S17-27 入账 reporter 并通过
