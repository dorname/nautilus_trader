# 任务清单：online-chat-llm-config-20261012

## [delta] 规格变更

- [x] `deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md` — 自由对话在线化 + 模型配置需求（MODIFIED 对应章节）
- [x] `deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md` — 右栏对话标注口径（在线/离线双态）+ 顶栏模型设置入口（MODIFIED）
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md` — 新场景 S22 时序图（自由对话派发 + 模型设置 upsert/test）
- [x] `deltas/test/core-S22-test-cases.md` — 新用例文件（UT + ST）
- [x] `deltas/test/core-S17-test-cases.md` — S17 兜底文案口径修订（MODIFIED）

## [code] 代码实现

### ① 自由对话接 octos
- [x] `app.rs`：意图路由兜底改真实 LLM 对话派发（任务条进度行 + 回复入消息流 + 项目上下文 prompt）
- [x] 诚实降级：octos 不可用回退 ai.rs 预设；底部标注双态（在线/离线）
- [x] 日志脱敏：对话内容/key 不进 logs

### ② LLM provider 配置
- [x] `oup.rs`：AppUI 方法族封装（catalog/list/upsert/select/test，类型化 params/result）
- [x] `app.rs`：顶栏「模型设置」按钮 + 浮层（primary 展示、catalog 下拉、route 表单、保存/测试）
- [x] api_key 安全口径：仅 RPC 内存传输，不落盘不进日志

### 测试
- [x] UT：OUP AppUI 序列化/解析、兜底改道、浮层校验与脱敏（reporter 入账）
- [x] ST：真实 LLM 自由对话 + provider upsert/list/test 闭环（reporter 入账）
- [x] 回归：S17 确认需求三路 / ST-GUI-01/02 / wire_real_llm

### 闭环
- [x] 本批覆盖用例 ID 与 test delta 对齐；verify 全量 PASS（Gate 3.6，覆盖 100% 156/156，154 通过 0 失败 4 跳过；首跑 st_m4_02 偶发 LLM 超时，单独重跑 6/6 后全量复跑通过）
