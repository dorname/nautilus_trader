## ADDED — 生产桌面在线对话与模型设置（online-chat-llm-config-20261012）

> 本节为生产桌面（`crates/research-desktop`）行为口径；上方「本轮边界」描述的是 HTML 演示原型本身，不随本节改动。

### 自由对话在线化

- 对话输入经意图路由：确定性业务动作（运行实验、生成报告、计划生成/核对/导出、新建项目等）保持本地执行不经 LLM；「生成策略设计 / 生成策略代码」维持既有真实 LLM 路径；**其余自由问答由真实 LLM 经 octos（OUP stdio）回复**，任务条出现「对话回复 ·」进度行，回复经 `assistant_persisted` 进入消息流。
- 非流式口径：turn 完成一次性回显；流式 delta 渲染为后续提案。
- 诚实降级：octos 不可用（profile 缺失 / 连接失败 / 版本不匹配）时回退离线预设意图引擎，右栏底部标注保持「离线预设交互 · 未接入模型」；接线可用时标注改为「AI 助手 · octos 已接线」。顶栏「演示环境」tag 保留（数据仍为合成样本，与模型接线无关）。

### 模型设置浮层

- 入口：顶栏「运行记录」旁新增「模型设置」按钮。
- 浮层内容：当前主模型（family / model / label）；catalog 拉取的 family、model 下拉；route 表单（label、base_url、api_type: openai/anthropic）；api_key 密码输入框（显示占位符，不回显）；操作：保存（upsert + set_primary）、连通测试、关闭。
- 安全：api_key 仅经 OUP RPC 内存传输（SecretString 语义），desktop 不写盘、不进日志文件、不入运行记录浮层。
- 生效口径：保存写入 octos profile，后续新 agent 会话生效；进行中的会话不热切换。
