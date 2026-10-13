# S23 对话隐式生成需求草稿（issue #39）

## 需求草稿派发（在线）

```mermaid
sequenceDiagram
    actor U as 用户
    participant G as 统一工作区
    participant R as 意图路由(ai.rs)
    participant A as AgentWorker
    participant O as octos serve
    U->>G: 对话描述研究想法并发送
    G->>R: 意图路由(关键词)
    alt 命中需求草稿意图(生成需求草稿/帮我生成需求等)
        R->>A: 派发草稿生成 turn(项目上下文+用户消息)
        A->>O: OUP turn(绝对路径 data/instance/workspace)
        O-->>A: turn/completed + assistant_persisted
        A-->>G: 草稿文本入需求草稿框(等用户编辑确认)
        Note over G: 任务条「生成需求草稿 ·」收尾；不自动确认版本
    else octos 不可用/生成空回复
        A-->>G: 降级:原文入草稿框+如实标注(沿用 S22 前行为)
    end
```

生产语义（fix-issues-38-39-20261013）：

- 草稿生成复用 AgentWorker 单活跃口径：有进行中任务时提示等待或取消，不并发派发。
- 草稿只填入草稿框（`ai_req_text`），需求版本确认仍由用户在需求页显式执行（草稿不自动生效）。
- 降级路径不丢用户输入：生成失败时原文保留入草稿框，回复中如实标注「未能接入模型」。
- 草稿内容写入 SQLite 工作区（既有持久化口径），不进 logs 日志文件。

## agent 会话路径口径（issue #38 修复）

- `AgentSessionConfig::desktop_with_profile` 与 `OupClient::spawn` 对数据/实例/工作区目录一律 canonicalize 为绝对路径后再使用：父进程复制 profile 的目标与子进程 `--data-dir` 读取位置必须一致。
- 背景：相对路径 + `Command.current_dir(workspace_root)` 使 `--data-dir` 在子进程内二次解析为嵌套路径，octos 在嵌套空目录初始化 profile store → `session/open` 报 `-32602 profile_unresolved`（详见 issue #38 复现矩阵）。
- 测试口径：新增相对路径回归用例（UT-S17-32），防止测试全用绝对路径掩盖该错位。

本地原型动作不构成生产 API。对应检查见 core-S23-test-cases.md 与 core-S17-test-cases.md。
