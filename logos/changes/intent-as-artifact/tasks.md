# 任务清单：intent-as-artifact

## [delta] 规格变更

- [x] `deltas/prd/1-product-requirements/core-09-ai-workspace-requirements.md` — 新增意图即 Artifact 需求与六类验收口径，修订对话主入口的确定性动作、诚实降级、确认门和日志边界
- [x] `deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md` — 增加系统处理流程图，补充 L0/L1/L2 入口、ClarificationRequested 候选按钮、needs_confirm 确认卡片、forbidden 阻断及离线标注交互
- [x] `deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md` — 增加意图转译架构图，明确意图注册表 schema、Triage 结构化转译、serde/注册表校验、一次错误回灌及确定性风险门架构约束
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S24-intent-as-artifact.md` — 新增 S24 场景时序图，覆盖显式命令、Triage 转译、重试/澄清、域外 ChatFree、确认/阻断和离线降级
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S22-online-chat-llm-config.md` — 将自由问答入口由「所有 Unknown 直接派发」修订为「Triage 判定域外后转 ChatFree」，保持流式与持久化产物口径
- [x] `deltas/prd/3-technical-plan/2-scenario-implementation/core-S23-requirement-draft-llm.md` — 将需求草稿入口改为 L0/L1 Artifact 分发，保留草稿失败降级、不自动确认版本及正文不进 logs 的边界
- [x] `deltas/test/core-S24-test-cases.md` — 新增 S24 UT/ST 用例：注册表与 Artifact 纯函数、六组离线评测、真实 LLM Triage、澄清、风险门、降级、日志与 reporter
- [x] `deltas/test/core-S17-test-cases.md` — 修订离线意图路由用例为 L0 显式命令与 14 个业务种子类型口径，去除宽泛 contains 路由的旧预期
- [x] `deltas/test/core-S22-test-cases.md` — 修订 UT-S22-02 等入口预期为域外 ChatFree/本地 Artifact 分发，保留模型配置、流式预览和任务进度回归口径
- [x] `deltas/test/core-S23-test-cases.md` — 对齐 Artifact 触发需求草稿后的 UT/ST 入口与降级预期，保留既有草稿归约及不自动确认验收

## [code] 代码实现

### ① 意图注册表与确定性层

- [ ] 新增 `logos/intents/*.toml`：录入现有 14 个业务意图的描述、边界反例、槽位、风险等级和示例，并加入真实下单类 forbidden 边界类型
- [ ] 在 research-desktop 增加注册表加载与校验代码：拒绝缺字段、重复/未知 ID、非法槽位或 risk_class，以及缺少确定性分发约束的可执行类型
- [ ] 收窄 `ai.rs` 的 `route()` 为芯片/精确显式命令匹配；实现 IntentArtifact、候选/置信度归约、注册表风险等级解析及确定性分发/确认/阻断纯函数
- [ ] 编写本批 S24 注册表、L0、Artifact 校验与风险门 UT，并接入 OpenLogos reporter；同步更新受影响的 S17/S22 纯函数用例

### ② Triage 转译、澄清与风险门交互

- [ ] 在 `app.rs` 新增 `AgentTaskKind::TriageTranslate`，复用 AgentWorker/OUP turn 通道实现注册表+项目摘要+用户原话 prompt、结构化结果消费和域外 ChatFree 改道
- [ ] 实现 serde 错误回灌且最多重试一次、再次失败或低置信/候选过近时澄清；在对话区实现 ClarificationRequested 候选意图按钮，不允许过期任务结果触发动作
- [ ] 将 needs_confirm 接入可复用确认卡片并区分确认/取消结果；在确定性执行层再次硬阻断 forbidden，保留既有业务前置校验
- [ ] 实现 octos 不可用/派发失败/空或非法回复的诚实降级与 UI 标注；确保 L0 离线可用，用户原文、识别正文、槽位值和重试上下文不进入运行记录或日志文件
- [ ] 编写本批 S24 状态归约与六组离线用例 UT、真实 LLM Triage/风险门/降级 ST，并接入 OpenLogos reporter；同步更新 S22/S23 入口与回归测试代码，保留流式回显和草稿降级边界
