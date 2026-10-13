# 任务清单：fix-issues-38-39-20261013

## [delta] 规格变更

- [x] `deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md` — 新增 FR-R23（需求草稿 LLM 生成）；边界段补 agent 会话路径绝对化口径（#38）
- [x] `deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md` — 需求草稿由 LLM 生成的交互口径（MODIFIED 在线对话节）
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S23-requirement-draft-llm.md` — 新场景 S23 时序图（对话描述 → LLM 草稿 → 用户确认；含 #38 路径口径生产语义）
- [x] `deltas/test/core-S23-test-cases.md` — 新用例文件（UT + ST）
- [x] `deltas/test/core-S17-test-cases.md` — ADDED UT-S17-32（agent 会话配置路径绝对化回归）

## [code] 代码实现

### ① #38 相对路径修复
- [x] `agent_bridge.rs`：`desktop_with_profile` 对 base_dir canonicalize（建目录后解析绝对路径）
- [x] `oup.rs`：spawn 传参前对 data_dir/instance_dir/workspace_root 防御性 canonicalize
- [x] UT-S17-32：相对路径输入 → 配置各路径为绝对路径（reporter 入账）

### ② #39 需求草稿 LLM 生成
- [x] `app.rs`：DraftRequirement 意图改道真实 LLM（AgentTaskKind::RequirementDraft + 草稿 prompt + 结果落草稿框；busy/失败诚实降级预设行为）
- [x] 草稿解析纯函数（非空/结构校验）+ UT-S23-01（reporter 入账）
- [x] ST-S23-01：真实 LLM 从一句自然语言生成需求草稿（reporter 入账；首跑 420s 超时边界，重跑 402s 通过）

### 回归
- [x] lib 全量 54/54 + gui_walkthrough 6/6 + s22_online_chat 2/2 + wire_real_llm 4/4（CPU 红线达标；ST-S22-02 遇 z.ai 端 HTTP 500 抖动重跑通过）
