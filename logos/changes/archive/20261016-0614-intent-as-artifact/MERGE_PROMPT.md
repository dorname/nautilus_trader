# 合并指令

## 变更提案
- 提案名称：intent-as-artifact
- 提案目录：logos/changes/intent-as-artifact/

## 提案内容

# 变更提案：意图即 Artifact（Intent-as-Artifact）

> module: core | created: 2026-10-14

## 变更原因

当前 AI 工作台以 `ai::route()` 的顺序敏感关键词匹配同时承担显式命令、业务动作和自由问答分流，存在五类问题：否定句误触发、宽泛关键词导致优先级脆弱、同义与口语表达漏识别、域内指令与域外闲聊混在 `Unknown`、低置信输入没有澄清路径。issue #42 已暴露自然语言需求表达与单独「确认」无法进入预期业务流程的问题。

本变更依据 `logos/design-drafts/20261014-intent-as-artifact.md` 第 4 章「提案一」，把自然语言入口改为「L0 显式命令 / L1 Triage 转译 / L2 诚实降级」，以声明式意图注册表和结构化 `IntentArtifact` 取代宽泛关键词分类。LLM 只提出识别建议，确定性代码负责注册表校验、风险判定和既有业务处理器分发。

## 变更类型

需求级（新增意图转译、澄清与统一风险门能力；同步修订 S17/S22/S23 的入口口径并新增 S24，无外部 API、数据库或部署变更）。

## 变更范围

- 影响的需求文档：`prd/1-product-requirements/core-09-ai-workspace-requirements.md`（新增意图即 Artifact 需求及验收口径，修订对话主入口边界段）。
- 影响的功能规格：`prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`（三层入口、澄清候选按钮、确认卡片、离线标注）。
- 影响的技术架构：`prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md`（意图注册表、Triage 转译、serde 校验回灌、确定性风险门；不引入 Artifact 黑板）。
- 影响的业务场景：S17（L0 显式命令口径）、S22（仅域外请求转入自由问答）、S23（需求草稿经 Artifact 进入既有生成路径）、新增 S24（意图转译、澄清与风险门）。
- 影响的测试规格：`core-S17-test-cases.md`、`core-S22-test-cases.md`、`core-S23-test-cases.md`，并新增 `core-S24-test-cases.md`。
- 影响的部署方案：无。
- 影响的 API：无外部 API；复用既有 AgentWorker 与 OUP `turn/start`、`turn/completed`、`turn/error` 通道，不新增 OUP 方法。
- 影响的 DB 表：无；不扩展 SQLite/redb schema，不在本提案把 Artifact 流转写入 `swarm_ledger`。
- 影响的编排测试：无（桌面项目无 API 编排）。
- 影响的 smoke 测试：无。

## 部署影响

- 是否需要部署：否
- 部署原因：仅修改本地桌面应用的意图识别、交互和测试，不涉及服务发布或环境变更。
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

1. **声明式意图注册表**：新增 `logos/intents/*.toml`，字段覆盖 `id / description / boundary_negative / slots / risk_class / examples`，`risk_class` 限定为 `local_action | needs_confirm | forbidden`。以现有 14 个业务意图为种子类型，`Unknown` 仍只承担兜底语义；另注册真实下单类禁止边界类型，用于识别后的硬阻断，不形成可执行能力。加载时校验必填字段、枚举、ID 唯一性、槽位声明及确定性分发映射。
2. **三层入口**：`ai::route()` 收窄为芯片和精确显式命令匹配器，命中后零模型直接构造 Artifact，离线仍可用。非显式输入在 octos 可用时新增 `AgentTaskKind::TriageTranslate`，复用现有 AgentWorker/OUP 派发模式，由模型根据注册表、项目状态摘要和用户原话输出结构化 JSON；octos 不可用时进入 L2，只保留 L0 和如实标注的离线能力说明。
3. **校验、重试与澄清**：Triage 输出须经 serde 与注册表联合校验，类型、槽位、枚举或 JSON 结构不合法时，把校验错误回灌并重试一次；再次失败产生应用内 `ClarificationRequested`，在对话区给出候选意图按钮。低置信或 top1/top2 过近同样澄清，阈值由用例集校准而非把模型自报置信度当作真实概率；明确域外的请求转既有 `ChatFree`。
4. **识别与执行解耦**：模型不得直接调用业务处理器。确定性代码以注册表中的风险等级为权威，校验并归约 Artifact 后再分发到既有处理器；`needs_confirm` 复用并泛化现有确认卡片，确认前不得执行，取消不得提交产物；`forbidden` 在注册表约束和确定性执行层双重硬阻断。注入文本不得改变风险等级或绕过确认门。
5. **兼容与评测**：保留 S22 流式预览/持久化落定、S23 草稿失败降级及不自动确认版本等既有边界。建立域内变体、否定句、歧义、域外、越权、注入六组离线用例；UT 锚定纯函数与状态归约，ST 使用真实 LLM 并遵循 `RESEARCH_OCTOS_PROFILE`，全部编号用例经 OpenLogos reporter 写入 `logos/resources/verify/test-results.jsonl`。

## 不可违背的验收红线

- **确定性动作优先，本地动作不经 LLM**。
- **octos 不可用时诚实降级并在 UI 如实标注，不编造理解**。
- **识别 ≠ 执行：LLM 只产出 Artifact 建议，执行由确定性代码与确认门控制**。
- **不自动下单：任何真实下单类意图为 forbidden，注册表层面硬阻断**。
- **草稿/识别内容不进日志文件**。用户原文、模型识别正文、Artifact 槽位值及含这些内容的重试上下文不得写入运行记录或日志文件；测试 reporter 也只记录用例 ID、状态和脱敏错误。

## 明确不在本次范围

- 不实现 Artifact 黑板、内存事件总线或 `swarm_ledger` 的 Artifact 持久化。
- 不扩展 `logos/agents/*.toml` 的 `consumes` / `publishes`，不实现多意图拆分、并行 fan-out 或订阅执行。
- 不引入语义订阅、fastembed-rs 或 octos 侧语义匹配。
- 不新增真实下单、自动交易或绕过现有业务前置校验的执行器；不让需求草稿自动确认版本。


## 需要合并的 Delta 文件

### 1. deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md`
- 目标目录：`logos/resources/prd/1-product-requirements/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`
- 目标目录：`logos/resources/prd/2-product-design/2-page-design/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md`
- 目标目录：`logos/resources/prd/3-technical-plan/1-architecture/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 5. deltas/prd/3-technical-plan/2-scenario-implementation/core-S23-requirement-draft-llm.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/prd/3-technical-plan/2-scenario-implementation/core-S23-requirement-draft-llm.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 6. deltas/prd/3-technical-plan/2-scenario-implementation/core-S24-intent-as-artifact.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/prd/3-technical-plan/2-scenario-implementation/core-S24-intent-as-artifact.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 7. deltas/test/core-S17-test-cases.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/test/core-S17-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 8. deltas/test/core-S22-test-cases.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/test/core-S22-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 9. deltas/test/core-S23-test-cases.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/test/core-S23-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 10. deltas/test/core-S24-test-cases.md

- Delta 文件：`logos/changes/intent-as-artifact/deltas/test/core-S24-test-cases.md`
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
   git add -A && git commit -m "docs(intent-as-artifact): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive intent-as-artifact`。
