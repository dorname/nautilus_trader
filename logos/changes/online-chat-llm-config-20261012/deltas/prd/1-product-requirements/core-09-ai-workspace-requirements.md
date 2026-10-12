## MODIFIED — 边界口径段（原文第 17 行段落）

对话主入口经 octos 接入真实模型（online-chat-llm-config-20261012）：意图路由保持确定性动作优先（实验、报告、计划等本地动作不经 LLM），自由问答由真实 LLM 经 OUP 接线回复；octos 不可用（profile 缺失/连接失败）时诚实降级为离线预设意图交互并在 UI 如实标注。合成样本不能提供真实市场、样本外或稳健性证据；正式验证保持“证据不足”。仅允许通过当前版本演示检查后生成明确标注的演示计划。

## ADDED — 模型配置需求（FR-R22 追加至需求表）

| 编号 | 需求 | 验收 |
|---|---|---|
| FR-R22 | GUI 内配置 octos LLM provider | 顶栏「模型设置」浮层：查看当前主模型、从 catalog 选择 family/model、填写 route（label/base_url/api_type）与 api_key、保存生效（后续新会话）、连通测试；api_key 仅经 RPC 内存传输，不写盘、不进日志；配置失败给出中文诊断 |
