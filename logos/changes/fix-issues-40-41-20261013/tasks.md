# 任务清单：fix-issues-40-41-20261013

## [delta] 规格变更

- [x] `deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md` — 新增 FR-R24（对话流式回显与进度细化）；边界段补路由画布状态隔离口径（#40）
- [x] `deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md` — MODIFIED 自由对话在线化节：非流式口径改为流式 delta 实时预览 + 终态以持久化产物为准；任务条进度细化（增量接收状态、多 Agent 单路状态）
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md` — MODIFIED 自由对话派发时序图：补 message/delta 增量通知与 GUI 实时预览回路
- [x] `deltas/test/core-S17-test-cases.md` — MODIFIED RD-018 段（落盘仍只取 assistant_persisted，新增展示层 delta 预览例外）+ ADDED delta 累积/终态替换 UT
- [x] `deltas/test/core-S22-test-cases.md` — ADDED 流式预览 UT/ST 用例
- [x] `deltas/test/core-S15-test-cases.md` — ADDED 路由画布状态隔离回归用例（#40）

## [code] 代码实现

### ① #40 路由状态隔离
- [ ] `app.rs`：ScrollArea id_salt 按路由隔离；画布内 TextEdit 显式唯一 ID
- [ ] 路由状态隔离回归 UT（reporter 入账）

### ② #41 流式 delta 渲染
- [ ] `oup.rs`：解析 message/delta 帧按 turn_id 累积增量
- [ ] `agent_bridge.rs`：run_turn 边收 delta 边推送 GUI
- [ ] `app.rs`：右栏消息流实时预览 + 终态以持久化产物替换；任务条流式进度
- [ ] 流式 UT（delta 累积/终态替换/降级）+ ST（真实 LLM 流式，reporter 入账）

### ③ #41 进度细化
- [ ] `app.rs`：多 Agent 任务条显示每路子任务状态
- [ ] 进度细化 UT（reporter 入账）

### 回归
- [ ] lib 全量 + gui_walkthrough + s22_online_chat + wire_real_llm 回归通过（CPU 红线达标）
