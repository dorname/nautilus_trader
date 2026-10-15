## MODIFIED — 自由对话派发（在线）

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
            A-->>G: 增量预览入消息流（标记「生成中」，任务条显示接收进度）
        end
        O-->>A: turn/completed + assistant_persisted
        A-->>G: 以持久化产物替换预览落定（RD-018：落盘禁止拼接 delta）
    end
    Note over G,O: octos 不可用（profile 缺失/连接失败）→ 未完成识别只进入 L2，<br/>L0 保持可用，UI 标注「离线预设交互 · 未接入模型」；<br/>ChatFree 已开始后的通道失败沿用 S22 诚实降级；<br/>octos 不发 delta 帧 → 降级一次性回显
```

自由问答的唯一自然语言入口是 **Triage 的 `domain=out_of_scope` 归约结果**；`Unknown` 不再直接等价于 ChatFree。Triage 中间 JSON 不使用 S22 的聊天流式预览，也不进入消息持久化；ChatFree 的流式、任务条与 `assistant_persisted` 权威口径不变。完整识别/重试/澄清与风险门时序见 `core-S24-intent-as-artifact.md`。
