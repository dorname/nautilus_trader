# 合并指令

## 变更提案
- 提案名称：fix-issues-40-41-20261013
- 提案目录：logos/changes/fix-issues-40-41-20261013/

## 提案内容

# 变更提案：修复 issue #40（路由间画布状态泄漏）与 #41（对话流式回显 + 进度细化）

> module: core | created: 2026-10-13

## 变更原因

- **issue #40（bug）**：切换到「回测实验」页时，画布上残留需求页 `TextEdit` 的需求草稿默认文本（`workspace.rs` `DRAFT_REQ_TEXT`）。根因：所有路由页面共用同一个 `ScrollArea` id_salt `"canvas-scroll"`（`app.rs:1284`），且需求页多行 `TextEdit`（`app.rs:2309`）未指定显式 ID，跨路由 widget 状态（滚动偏移 / 文本编辑状态）在 egui 内存中按相同 Id 共享。无头 `egui_kittest` 无法复现（依赖真实 GPU 渲染路径 / 窗口尺寸 / focus 状态），现有 UT-S15-07「切页滚动保留互不串扰」口径被违反但无针对性回归用例。
- **issue #41（体验缺陷 + 增强）**：对话与多 Agent 任务等待时间长且期间无任何中间反馈。确认口径：当前为非流式——`AgentWorker::run_turn` 以 100ms 轮询 `poll_turn_outcome`，直到 `turn/completed` 终态才把完整 `assistant_text` 一次性推给 GUI（core-05 设计文档明确「非流式口径：turn 完成一次性回显；流式 delta 渲染为后续提案」）。实证：策略生成类 turn 单次 LLM iteration 可达 3.5 分钟。octos 协议已存在 `message/delta` 增量帧，desktop 侧未订阅。多 Agent 任务条「0/3」只在子任务全部成功时才跳动，执行中无单路状态。

## 变更类型

需求级（小范围：一个 UI 状态隔离修复 + 一个对话流式回显增强；无 API/DB/部署变更）

## 变更范围

- 影响的需求文档：`prd/1-product-requirements/core-09-ai-workspace-requirements.md`（新增 FR-R24 对话流式回显与进度细化；边界段补路由画布状态隔离口径）
- 影响的功能规格：`prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`（在线对话节：非流式口径改为流式 delta 实时预览 + 终态持久化口径；任务条进度细化）
- 影响的业务场景：S15（路由导航画布状态隔离回归用例）、S17（RD-018 持久化文本口径修订——落盘仍只取 `assistant_persisted`，新增展示层 delta 预览例外）、S22（自由对话时序图补 message/delta 增量流）
- 影响的部署方案：无
- 影响的 API：无（OUP 协议 `message/delta` 为既有服务端帧，desktop 仅新增消费订阅，不改契约）
- 影响的 DB 表：无
- 影响的编排测试：无（桌面项目无 API 编排）
- 影响的 smoke 测试：无

## 部署影响

- 是否需要部署：否
- 部署原因：纯桌面本地功能修复与体验增强，无服务部署
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

1. **#40 路由状态隔离**：`render_canvas` 的 `ScrollArea` id_salt 按路由隔离（如 `canvas-scroll-{route}`），配合既有 `last_route`/`scroll_of` 每路由滚动恢复机制；需求页等画布内 `TextEdit` 显式指定路由命名空间下的唯一 ID，杜绝跨路由 widget 状态共享。新增回归用例（静态检查 id 唯一性 + kittest 遍历断言泄漏 marker 不出现）。
2. **#41 流式 delta 渲染**：`OupClient` 在 turn 事件泵中解析 `message/delta` 帧按 turn_id 累积增量；`AgentWorker` 边收 delta 边经既有 channel 推送 GUI；右栏消息流实时追加预览文本，收到 `turn/completed` 后以 `assistant_persisted` 持久化产物为准替换落盘（RD-018 落盘口径不变，delta 仅用于展示预览）。octos 未发 delta 帧时诚实降级为现有一次性回显。
3. **#41 进度细化**：对话任务条在流式期间显示接收状态（已收增量字数）；多 Agent 任务条在 0/3 之外显示每路子任务状态（执行中/已完成/失败）。LLM iteration 计数不在范围（OUP 通知帧不暴露 iteration 元数据，仅 octos serve 日志可见）。
4. **边界不变**：确定性动作不经 LLM、需求版本需用户确认、草稿生成（S23）结构化结果一次性入框不做逐字预览、工具调用结果仍等终态统一显示——均不随本次变更松动。


## 需要合并的 Delta 文件

### 1. deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md

- Delta 文件：`logos/changes/fix-issues-40-41-20261013/deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md`
- 目标目录：`logos/resources/prd/1-product-requirements/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md

- Delta 文件：`logos/changes/fix-issues-40-41-20261013/deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`
- 目标目录：`logos/resources/prd/2-product-design/2-page-design/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md

- Delta 文件：`logos/changes/fix-issues-40-41-20261013/deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/test/core-S15-test-cases.md

- Delta 文件：`logos/changes/fix-issues-40-41-20261013/deltas/test/core-S15-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 5. deltas/test/core-S17-test-cases.md

- Delta 文件：`logos/changes/fix-issues-40-41-20261013/deltas/test/core-S17-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 6. deltas/test/core-S22-test-cases.md

- Delta 文件：`logos/changes/fix-issues-40-41-20261013/deltas/test/core-S22-test-cases.md`
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
   git add -A && git commit -m "docs(fix-issues-40-41-20261013): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive fix-issues-40-41-20261013`。
