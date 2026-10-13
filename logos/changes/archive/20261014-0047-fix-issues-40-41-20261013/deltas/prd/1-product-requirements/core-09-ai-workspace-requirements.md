## MODIFIED — 边界口径段（原文「对话主入口经 octos 接入真实模型」段）

对话主入口经 octos 接入真实模型（online-chat-llm-config-20261012；fix-issues-38-39-20261013、fix-issues-40-41-20261013 修订）：意图路由保持确定性动作优先（实验、报告、计划等本地动作不经 LLM），自由问答由真实 LLM 经 OUP 接线回复，**回复以 message/delta 增量实时预览、turn 终态以 `assistant_persisted` 持久化产物落定**（issue #41；RD-018 落盘口径不变）；**需求草稿意图（DraftRequirement）由真实 LLM 隐式生成结构化草稿**（issue #39）；octos 不可用（profile 缺失/连接失败）时诚实降级为离线预设意图交互并在 UI 如实标注。**agent 会话与 LLM 配置的数据/实例/工作区路径一律绝对化（canonicalize）后传入 octos 子进程**（issue #38：相对路径在子进程 cwd 下二次解析致 profile store 错位）。**各路由画布的滚动容器与文本编辑控件使用路由命名空间下的唯一 widget ID，切换路由不得共享 egui 持久化状态**（issue #40：全路由共用 id_salt 致需求草稿文本泄漏至实验页）。合成样本不能提供真实市场、样本外或稳健性证据；正式验证保持“证据不足”。仅允许通过当前版本演示检查后生成明确标注的演示计划。

## ADDED — 对话流式回显与进度细化需求（FR-R24 追加至需求表）

| 编号 | 需求 | 验收 |
|---|---|---|
| FR-R24 | 对话流式回显与任务进度细化（issue #41） | 自由对话及 LLM 任务执行期间，右栏消息流以 message/delta 增量实时预览回复并标记「生成中」；turn 终态后以 `assistant_persisted` 持久化产物为准替换落定，预览与持久化不一致时以持久化为准；octos 未发 delta 帧时诚实降级为一次性回显；对话任务条显示增量接收状态；多 Agent 任务条在终态计数之外显示每路子任务状态（执行中/已完成/失败） |
