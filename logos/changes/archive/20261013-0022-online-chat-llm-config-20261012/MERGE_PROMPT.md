# 合并指令

## 变更提案
- 提案名称：online-chat-llm-config-20261012
- 提案目录：logos/changes/online-chat-llm-config-20261012/

## 提案内容

# 变更提案：自由对话接入 octos + LLM provider 配置

> module: core | created: 2026-10-12

## 变更原因

1. **自由对话未接线**：研究助手对话框的通用问答走 `ai.rs` 离线预设意图引擎（无 LLM），未命中意图时只能回兜底文案；UI 标注「未接入模型」。用户要求自由对话接入 octos 真实 LLM。
2. **LLM provider 不可配置**：desktop 复制既有 octos profile 使用，无法在 GUI 内查看/切换/新增 provider。octos 服务端（2.0.3-rc.9）已在 UI 协议通道提供 AppUI 方法族（`profile/llm/catalog|list|upsert|delete|select|test`，`ui_protocol_transport.rs`，与 octoscode 同源契约），desktop 具备直接接线条件。

## 变更类型

设计级变更（产品口径变化：离线预设 → 在线助手；新增模型设置 UI）

## 变更范围

- 影响的需求文档：`prd/1-product-requirements/core-09-ai-workspace-requirements.md`（自由对话在线化 + 模型配置需求）
- 影响的功能规格：`prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`（右栏对话标注口径、设置入口）
- 影响的业务场景：新增 **S22**（AI 助手在线对话与模型配置）；S17（自由对话路径修订）
- 影响的部署方案：无
- 影响的 API：无（经既有 OUP stdio 通道，不改协议）
- 影响的数据库：无（provider 配置存 octos profile，由 octos 管理）
- 影响的测试用例：新增 `test/core-S22-test-cases.md`；`test/core-S17-test-cases.md` 修订
- 影响的代码：
  - `crates/research-desktop/src/oup.rs`（AppUI LLM 方法族封装：catalog/list/upsert/select/test）
  - `crates/research-desktop/src/app.rs`（自由对话兜底改真实 LLM 派发；模型设置浮层；标注口径）
  - `crates/research-desktop/src/agent_bridge.rs`（通用对话任务类型，如需要）
  - `crates/research-desktop/src/ai.rs`（保留为离线降级路径，标注调整）

## 部署影响

- 是否需要部署：否
- 部署原因：桌面端功能增强，无部署物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

**① 自由对话接 octos**：
- `do_ai_send_text` 意图路由保持「确定性动作优先」（实验/报告/计划等本地动作不变）；未命中意图的自由问答改为派发真实 LLM 对话 turn（复用 AgentWorker 单活跃口径，任务条出现「对话回复 ·」进度行），`assistant_persisted` 回复入消息流。
- prompt 带轻量项目上下文（当前项目名 + 活跃需求/设计摘要），非流式回显（流式 delta 留后续提案）。
- 诚实降级：octos 不可用（profile 缺失/连接失败）时回退 `ai.rs` 预设引擎，UI 标注保持「离线预设」；接线可用时底部标注改为「AI 助手 · octos 已接线」。
- api_key 等敏感信息不进日志（logging 脱敏口径沿用：日志只记任务标签与状态）。

**② LLM provider 配置（参考 octoscode 后端实现）**：
- OupClient 封装 AppUI 方法族：`llm_catalog()`（provider/模型目录）、`llm_list()`（已配置列表）、`llm_upsert(selection, api_key, set_primary)`、`llm_select(family, model)`、`llm_test(...)`（连通测试）。
- GUI：顶栏新增「模型设置」按钮 → 浮层：当前 primary 展示、catalog 选择（family/model 下拉）、route 表单（label/base_url/api_type/api_key 密码框）、保存（upsert+set_primary）、连通测试按钮。
- 安全口径：api_key 仅经 RPC 内存传输（SecretString 语义），desktop 不写盘、不进日志、不入运行记录。
- 配置生效路径：upsert 写 octos profile → 后续新 agent 会话生效；进行中的会话不热切换（口径写明）。

**不在范围内**：流式回复（message/delta 实时渲染）、fallback 多档配置 UI（仅 primary + upsert）、octos web/admin bundle、网关参数（max_iterations 等）配置。

## 验收口径

- 新增 UT：OUP AppUI 请求序列化与响应解析、意图路由兜底改道、设置浮层表单校验与 key 脱敏
- 新增 ST（真实 LLM）：自由对话收到非预设回复；provider upsert → list 可见 → test 连通闭环
- S17 既有用例回归（确认需求三路/AI 整理参数不受影响）
- `openlogos verify` Gate PASS（CPU 红线：CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1）


## 需要合并的 Delta 文件

### 1. deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md

- Delta 文件：`logos/changes/online-chat-llm-config-20261012/deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md`
- 目标目录：`logos/resources/prd/1-product-requirements/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md

- Delta 文件：`logos/changes/online-chat-llm-config-20261012/deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`
- 目标目录：`logos/resources/prd/2-product-design/2-page-design/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md

- Delta 文件：`logos/changes/online-chat-llm-config-20261012/deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/test/core-S17-test-cases.md

- Delta 文件：`logos/changes/online-chat-llm-config-20261012/deltas/test/core-S17-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 5. deltas/test/core-S22-test-cases.md

- Delta 文件：`logos/changes/online-chat-llm-config-20261012/deltas/test/core-S22-test-cases.md`
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
   git add -A && git commit -m "docs(online-chat-llm-config-20261012): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive online-chat-llm-config-20261012`。
