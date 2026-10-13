# 任务清单：fix-issues-38-39-20261013

## [delta] 规格变更

- [x] `deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md` — 新增 FR-R23（需求草稿 LLM 生成）；边界段补 agent 会话路径绝对化口径（#38）
- [x] `deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md` — 需求草稿由 LLM 生成的交互口径（MODIFIED 在线对话节）
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S23-requirement-draft-llm.md` — 新场景 S23 时序图（对话描述 → LLM 草稿 → 用户确认；含 #38 路径口径生产语义）
- [x] `deltas/test/core-S23-test-cases.md` — 新用例文件（UT + ST）
- [x] `deltas/test/core-S17-test-cases.md` — ADDED UT-S17-32（agent 会话配置路径绝对化回归）

## [code] 代码实现

### ① #38 相对路径修复
- [ ] `agent_bridge.rs`：`desktop_with_profile` 对 base_dir canonicalize（建目录后解析绝对路径）
- [ ] `oup.rs`：spawn 传参前对 data_dir/instance_dir/workspace_root 防御性 canonicalize
- [ ] UT-S17-32：相对路径输入 → 配置各路径为绝对路径（reporter 入账）

### ② #39 需求草稿 LLM 生成
- [ ] `app.rs`：DraftRequirement 意图改道真实 LLM（AgentTaskKind::RequirementDraft + 草稿 prompt + 结果落草稿框；busy/失败诚实降级预设行为）
- [ ] 草稿解析纯函数（非空/结构校验）+ UT-S23-01（reporter 入账）
- [ ] ST-S23-01：真实 LLM 从一句自然语言生成需求草稿（reporter 入账）

### 回归
- [ ] lib 全量 + gui_walkthrough + s22_online_chat + wire_real_llm（CPU 红线 CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1）
