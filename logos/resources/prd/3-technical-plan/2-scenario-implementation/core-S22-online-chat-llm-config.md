# S22 AI 助手在线对话与模型配置

## 自由对话派发（在线）

```mermaid
sequenceDiagram
    actor U as 用户
    participant G as 统一工作区
    participant R as L0 意图路由(ai.rs)
    participant A as AgentWorker
    participant O as octos serve
    U->>G: 对话输入并发送
    G->>R: L0 芯片/精确显式命令匹配
    alt L0 命中业务意图
        R->>G: 直接构造 Artifact（零模型）
        Note over G: 进入 S24 确定性风险门；本地业务执行不经 LLM，<br/>设计/代码/草稿等内容生成走各自既有任务路径
    else L0 未命中，Triage 判定域内
        G->>A: TriageTranslate（注册表+项目摘要+用户原话）
        A->>O: OUP turn（结构化 JSON 输出）
        O-->>A: turn/completed + assistant_persisted
        A-->>G: 合法 Artifact / ClarificationRequested
        Note over G: Artifact 进入 S24 风险门；澄清等待显式选择，<br/>均不得悄悄改道自由问答
    else L0 未命中，Triage 明确判定域外
        G->>A: TriageTranslate → domain=out_of_scope
        A->>O: OUP turn（版本 pin 匹配）
        O-->>A: assistant_persisted（域外判定）
        G->>A: 串行派发 ChatFree（项目上下文+用户原话）
        A->>O: OUP turn
        loop 自由对话生成期间（issue #41 流式）
            O-->>A: message/delta（增量文本）
            A-->>G: 增量预览入消息流（标记「生成中」,任务条显示接收进度）
        end
        O-->>A: turn/completed + assistant_persisted
        A-->>G: 以持久化产物替换预览落定（RD-018:落盘禁止拼接 delta）
    end
    Note over G,O: octos 不可用（profile 缺失/连接失败）→ 未完成识别只进入 L2，<br/>L0 保持可用，UI 标注「离线预设交互 · 未接入模型」；<br/>ChatFree 已开始后的通道失败沿用 S22 诚实降级；<br/>octos 不发 delta 帧 → 降级一次性回显
```

自由问答的唯一自然语言入口是 **Triage 的 `domain=out_of_scope` 归约结果**；`Unknown` 不再直接等价于 ChatFree。Triage 中间 JSON 不使用 S22 的聊天流式预览，也不进入消息持久化；ChatFree 的流式、任务条与 `assistant_persisted` 权威口径不变。完整识别/重试/澄清与风险门时序见 `core-S24-intent-as-artifact.md`。

## 模型设置（provider 配置，参考 octoscode 后端）

```mermaid
sequenceDiagram
    actor U as 用户
    participant G as 模型设置浮层
    participant C as OupClient
    participant O as octos serve
    U->>G: 顶栏「模型设置」
    G->>C: profile/llm/list + profile/llm/catalog
    C->>O: AppUI RPC(OUP stdio)
    O-->>G: 当前 primary + 可用 family/model 目录
    U->>G: 选择 family/model,填 route 与 api_key
    U->>G: 连通测试
    G->>C: profile/llm/test
    C->>O: RPC(api_key 仅内存传输)
    O-->>G: 连通结果(成功/中文诊断)
    U->>G: 保存
    G->>C: profile/llm/upsert(selection, api_key, set_primary=true)
    C->>O: RPC 写 profile
    O-->>G: applied 确认
    G->>U: 生效提示(后续新会话使用)
```

生产语义（online-chat-llm-config-20261012；fix-issues-40-41-20261013 修订）：

- 自由对话 turn 复用 AgentWorker 单活跃口径：有进行中任务时提示等待或取消，不并发派发。
- **流式回显（issue #41）**：turn 执行期间 `message/delta` 增量帧按 turn_id 归集，实时推入右栏消息流作预览（标记「生成中」）；`turn/completed` 到达后以 `assistant_persisted` 持久化产物替换落定，二者不一致以持久化为准；octos 未发 delta 时降级一次性回显。delta 只进展示预览，不进持久化、不进日志、不进版本化产物。
- api_key 仅经 RPC 内存传输：desktop 不写盘、不进 logs、不入运行记录；浮层不回显已存 key。
- 配置生效于后续新 agent 会话；进行中会话不热切换。
- octos 服务端校验严格（`deny_unknown_fields`，#2166）：客户端只发送契约内字段，未知字段会被 typed error 拒绝，UI 需原样呈现诊断。

本地原型动作不构成生产 API。对应检查见 core-S22-test-cases.md。
