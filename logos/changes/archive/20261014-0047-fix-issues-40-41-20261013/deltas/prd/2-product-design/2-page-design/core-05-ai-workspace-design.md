## MODIFIED — 生产桌面在线对话与模型设置（online-chat-llm-config-20261012）
> 本节为生产桌面（`crates/research-desktop`）行为口径；上方「本轮边界」描述的是 HTML 演示原型本身，不随本节改动。

### 自由对话在线化

- 对话输入经意图路由：确定性业务动作（运行实验、生成报告、计划生成/核对/导出、新建项目等）保持本地执行不经 LLM；「生成策略设计 / 生成策略代码」维持既有真实 LLM 路径；**其余自由问答由真实 LLM 经 octos（OUP stdio）回复**，任务条出现「对话回复 ·」进度行，回复以 `message/delta` 增量实时预览、turn 终态经 `assistant_persisted` 进入消息流。
- **需求草稿意图在线化（S23 / issue #39）**：命中「生成需求草稿 / 帮我生成需求」意图时，由真实 LLM 以用户消息为输入隐式生成结构化需求草稿（研究目标、范围、参数、验收标准），任务条出现「生成需求草稿 ·」进度行；草稿填入需求页草稿框等用户编辑确认（不自动确认版本）。生成失败或 octos 不可用时降级为原文入草稿框（S22 前行为）并在回复中如实标注。**草稿为结构化结果，一次性入框，不做逐字流式预览**。
- **流式口径（issue #41，取代原「非流式口径」）**：turn 执行期间 desktop 订阅 `message/delta` 增量帧，右栏消息流实时追加预览文本并标记「生成中」；收到 `turn/completed` 后以 `assistant_persisted` 持久化产物为准替换落定，预览与持久化文本不一致时以持久化为准。RD-018 落盘口径不变：回复文本的持久化来源仍只是 `assistant_persisted`，禁止以 delta 拼接结果落盘或进入版本化产物。octos 未发 delta 帧（profile/版本不支持）时诚实降级为一次性回显。工具调用结果仍等终态统一显示，不做半截渲染。
- **任务条进度细化（issue #41）**：流式期间对话任务条显示「对话回复 · 接收中 · Ns」及已收增量字数；多 Agent 任务条在「R{n} · {done}/3」终态计数之外显示每路子任务状态（执行中/已完成/失败）。LLM iteration 计数不做——OUP 通知帧不暴露 iteration 元数据（仅 octos serve 日志可见）。
- 诚实降级：octos 不可用（profile 缺失 / 连接失败 / 版本不匹配）时回退离线预设意图引擎，右栏底部标注保持「离线预设交互 · 未接入模型」；接线可用时标注改为「AI 助手 · octos 已接线」。顶栏「演示环境」tag 保留（数据仍为合成样本，与模型接线无关）。

### 模型设置浮层

- 入口：顶栏「运行记录」旁新增「模型设置」按钮。
- 浮层内容：当前主模型（family / model / label）；catalog 拉取的 family、model 下拉；route 表单（label、base_url、api_type: openai/anthropic）；api_key 密码输入框（显示占位符，不回显）；操作：保存（upsert + set_primary）、连通测试、关闭。
- 安全：api_key 仅经 OUP RPC 内存传输（SecretString 语义），desktop 不写盘、不进日志文件、不入运行记录浮层。
- 生效口径：保存写入 octos profile，后续新 agent 会话生效；进行中的会话不热切换。
- **路径口径（issue #38）**：agent 会话与 LLM 配置使用的数据/实例/工作区目录在传入 octos 子进程前一律 canonicalize 为绝对路径；相对路径在子进程 cwd 下二次解析会导致 profile store 错位（session/open -32602）。

## ADDED — 路由画布状态隔离（issue #40）

- 各路由画布的滚动容器（ScrollArea）按路由命名 id_salt（如 `canvas-scroll-{route}`），禁止全路由共用同一 id_salt。
- 画布内文本编辑控件（TextEdit 等持有 egui 持久化状态的 widget）显式指定路由命名空间下的唯一 ID，禁止依赖按位置自动生成的默认 ID。
- 切换路由时 widget 持久化状态（滚动偏移、文本编辑状态）不跨路由共享；滚动偏移按既有每路由 `scroll_of` 机制恢复。
- 回归由 UT-S15-13 承载（静态 ID 命名空间检查 + kittest 往返泄漏防御断言）。
