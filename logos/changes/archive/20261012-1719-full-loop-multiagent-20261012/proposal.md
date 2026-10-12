# 变更提案：2026-10-12 全功能多 Agent 业务测试问题归档

> module: core | created: 2026-10-12

## 变更原因

执行「本地启动，全功能、全流程、多 agent 业务测试」：

**已通过（摘要）**
- CLI S21：8/8
- OUP M1：5/5
- S17–S20 AI 工作台（含 ST-S17-16）：6/6
- S15 L2/L3：17/17
- wire_real_llm：4/4（含 ST-S17-17/18、ST-S18-17）
- m2 多 Agent Swarm：15/15（含 ST-S17-19/20/21/22/23）
- m4 实验：6/6（含 ST-S19-15）
- 原生 GUI X11：窗口可枚举，窗口像素非黑 98.3%

**失败并已提 Issue**
- [#36](https://github.com/dorname/nautilus_trader/issues/36)：`gui_walkthrough` ST-GUI-01/02 旅程结束后 SIGSEGV（可稳定复现）
- [#37](https://github.com/dorname/nautilus_trader/issues/37)：`st_m3_01_pipeline_dispatch` 偶发 design stage `assistant_persisted` 为空（同套件 ST-S18-16 通过；单测复跑可通过）

需同步本地 `docs/research-desktop/issues/` 归档。

## 变更类型

代码级（文档归档 / 问题跟踪；不改业务代码、不改 logos/resources 规格）

## 变更范围

- 影响的文档：`docs/research-desktop/issues/`（新增 RD-011/RD-012 或等价条目 + README 摘要）
- 其余 logos/resources / 代码：无

## 部署影响

- 是否需要部署：否
- 是否涉及数据迁移：否
- 是否需要 smoke：否

## 变更概述

归档全量测试矩阵结果与两个新 GitHub Issue；不修复代码（修复需另开变更提案）。

## 不在范围内

- 修复 SIGSEGV / Pipeline 空产物
- 重建 octos web/admin bundle
- 修改 research-desktop 业务代码
