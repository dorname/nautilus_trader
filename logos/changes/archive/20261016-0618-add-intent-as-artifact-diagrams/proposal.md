# 变更提案：add-intent-as-artifact-diagrams

> module: core | created: 2026-10-16

## 变更原因
`intent-as-artifact` 提案已合并并通过 verify，但产品设计师在复盘时发现 `core-05-ai-workspace-design.md` 的「意图即 Artifact」章节仅包含系统处理流程图，缺少高层架构图与入口识别时序图。为保持 OpenLogos「Why → What → How」三层推进模型中设计文档的完整性，需补充上述两类图，使后续维护者能直接从设计文档理解组件边界、信任边界与交互时序。

## 变更类型
设计级

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：`logos/resources/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`
- 影响的业务场景：S24 意图即 Artifact
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（纯文档补充）

## 部署影响
- 是否需要部署：否
- 部署原因：仅补充设计文档，不改代码与配置
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述
在 `core-05-ai-workspace-design.md` 的「意图即 Artifact 交互与系统处理流程（S24）」小节中，新增：
1. 架构图：展示用户、research-desktop、AgentWorker、octos serve/LLM、意图注册表、确定性风险门、既有业务处理器、SQLite 与研究协调器之间的组件关系与信任边界。
2. 时序图：展示 L0 命中、L1 Triage、校验重试、域外 ChatFree、澄清候选、Artifact 生成及风险门执行的完整交互时序。

系统处理流程图已存在于同小节，本次保持不变。
