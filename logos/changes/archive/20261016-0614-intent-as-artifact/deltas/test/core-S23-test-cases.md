## MODIFIED — 验收条件

- **S23-AC-01**：需求草稿经 L0 显式选择或 L1 合法 `requirement.draft.requested` Artifact 进入真实 LLM 生成路径，结构化草稿填入草稿框；草稿生成失败/不可用时降级原文入框并如实标注，Triage 未识别时走 L2 且不擅自入框；草稿不自动确认版本。
- **S23-AC-02**：agent 会话配置路径绝对化（issue #38）：相对路径输入经 canonicalize 后为绝对路径，父子进程路径视图一致。

## MODIFIED — 单元用例（无 LLM，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S23-01 | S23-AC-01 | 草稿结果归约纯函数：非空草稿入框；空/空白回复判失败 | 非空草稿写入 `ai_req_text` 且提示「等编辑确认」；空白回复返回失败语义（调用方走降级原文入框）；草稿内容不进入运行记录 |
| UT-S23-02 | S23-AC-01 | Artifact 入口与分阶段降级（无头状态归约，无 worker） | 仅 L0/L1 的 DraftRequirement Artifact 允许启动后续草稿路径，ChatFree/ClarificationRequested/其他 Artifact 不写草稿；Triage 通道失败只给 L2 且原草稿不变；已识别后的生成失败才把本次原文入框，任何路径均不自动确认 R 版本 |

## MODIFIED — 场景用例（真实 LLM / 真实 octos，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| ST-S23-01 | S23-AC-01 | 真实 LLM 两阶段链路：以「生成康欣新材的量价需求」先经生产 Triage 转译，合法识别后串行派发需求草稿 prompt | Triage 得到 `requirement.draft.requested` 且为 local_action；草稿 turn/completed 终态，回复非空且含结构化要素（目标/范围/参数/验收中至少三类关键词），不含离线预设兜底文案；草稿只入框，需求版本数不增加、不自动确认 |

### reporter

UT 纯本地断言（纯函数/无头状态）；ST 的 Triage 与草稿生成均为真实 LLM 调用并遵循 RESEARCH_OCTOS_PROFILE 配置选择。结果经共享 reporter 写入 test-results.jsonl，fail 含脱敏 error，skip 不可计为通过；不记录用户原文、Triage JSON 或草稿正文。
