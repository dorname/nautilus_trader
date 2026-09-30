# S17 需求到协作产物

需求来源：core-09-ai-workspace-requirements.md；交互详见 core-05-ai-workspace-design.md。

```mermaid
sequenceDiagram
    actor U as 研究员
    participant G as 工作台
    participant W as 协调器
    U->>G: 确认需求版本
    G->>G: 校验与冻结上游引用
    G->>W: 提交当前版本任务
    W-->>G: 进度与可观察事件
    alt 成功
        W-->>G: 角色产物
        G-->>U: 展示版本、依据与后续入口
    else 取消或校验失败
        G-->>U: 保留历史，展示错误或取消状态
    end
```

本轮仅实现本地内存交互；无生产接口新增。过期输入不能启动新实验，已有结果仍可查看。测试见 core-S17-test-cases.md。

工程台重构后页面映射：需求页承载需求文档维护（正文、验收标准、结构化参数与版本历史），Agent 协作并入该页侧栏的协作记录。
