## ADDED — 意图即 Artifact 需求（FR-R25 追加至需求表）

| 编号 | 需求 | 验收 |
|---|---|---|
| FR-R25 | 意图即 Artifact：声明式注册表、Triage 转译与确定性风险门（issue #42 后续） | `logos/intents/*.toml` 以现有 14 个业务意图为种子并声明 `id/description/boundary_negative/slots/risk_class/examples`；芯片与精确指令由 L0 零模型构造 Artifact，其他输入由真实 LLM 经 `TriageTranslate` 输出结构化候选；serde/注册表校验失败回灌重试一次，再失败或低置信/候选过近时澄清并提供候选按钮，域外请求才转 ChatFree；模型不直接执行动作，`needs_confirm` 经确认卡片放行，`forbidden` 在注册表与确定性执行层硬阻断；octos 不可用时仅保留 L0 与如实标注的离线降级；域内变体/否定句/歧义/域外/越权/注入六组用例有 UT/ST 证据；草稿、用户原文、识别正文及槽位值不进日志文件 |

## MODIFIED — 边界口径段（原文「对话主入口经 octos 接入真实模型」段）

对话主入口经 octos 接入真实模型（online-chat-llm-config-20261012；fix-issues-38-39-20261013、fix-issues-40-41-20261013、intent-as-artifact 修订）：意图入口分为 **L0 芯片/精确显式命令、L1 Triage 结构化转译、L2 离线诚实降级**。确定性动作优先，本地业务动作的执行不经 LLM；L1 仅把非显式自然语言转译为 Artifact 建议，只有判定域外的请求才由真实 LLM 经 OUP 进入自由问答。**回复以 message/delta 增量实时预览、turn 终态以 `assistant_persisted` 持久化产物落定**（issue #41；RD-018 落盘口径不变）；**需求草稿意图（DraftRequirement）经 L0/L1 Artifact 进入既有真实 LLM 结构化草稿路径**（issue #39），草稿仍不自动确认版本。octos 不可用（profile 缺失/连接失败）时，尚未完成识别不得编造意图或擅自填入产物，只保留 L0 并在 UI 如实标注；已完成 DraftRequirement 识别后的草稿生成失败仍按 S23 保留原文入框。**所有 `needs_confirm` 动作必须经人类确认，真实下单类意图始终 `forbidden`；识别与执行分离，模型输出不得修改注册表风险等级或绕过业务前置校验。**

**agent 会话与 LLM 配置的数据/实例/工作区路径一律绝对化（canonicalize）后传入 octos 子进程**（issue #38：相对路径在子进程 cwd 下二次解析致 profile store 错位）。**各路由画布的滚动容器与文本编辑控件使用路由命名空间下的唯一 widget ID，切换路由不得共享 egui 持久化状态**（issue #40：全路由共用 id_salt 致需求草稿文本泄漏至实验页）。**草稿/识别内容不进日志文件**：用户原文、模型识别正文、Artifact 槽位值及含这些内容的重试上下文只在必要内存/既有工作区边界内使用，不写运行记录或 logs。合成样本不能提供真实市场、样本外或稳健性证据；正式验证保持“证据不足”。仅允许通过当前版本演示检查后生成明确标注的演示计划，不自动下单。
