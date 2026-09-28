# D-06 · 崩溃恢复（处理流程图）

> Crash-only 设计：无优雅停机路径，恢复 = 事件重放重建状态。

```mermaid
flowchart TB
    RUN["节点正常运行<br/>（事件持续写入 event_store）"] --> CRASH(["任意时刻崩溃<br/>（kill -9 / 断电 / panic）"])
    CRASH --> START["进程重启<br/>（守护系统拉起）"]
    START --> LOAD["加载配置 + 事件存储<br/>（event_store / backing）"]
    LOAD --> REPLAY["按序重放历史事件流"]
    REPLAY --> REBUILD["重建 Cache 状态<br/>订单 · 仓位 · 账户 · 工具"]
    REBUILD --> PORTF["Portfolio 基于重放事件<br/>重算盈亏/敞口"]
    PORTF --> VER{"恢复后状态<br/>一致性校验"}
    VER -- "不一致" --> ALERT["告警 + 人工介入<br/>（暂停交易）"]
    VER -- "一致" --> RESUME["恢复对外连接<br/>（WS/REST）"]
    RESUME --> RECON["与 venue 对账<br/>（见 D-09：崩溃窗口内<br/>的回报可能未达引擎）"]
    RECON --> LIVE["继续正常运行"]
```

**关键设计**
- **无停机协议**：不实现"优雅关闭"特殊路径——崩溃即常态，任何时刻可恢复（NFR-004）
- **事件溯源为唯一真相**：状态可全量由事件导出，Cache 只是投影
- **恢复后必对账**：崩溃窗口内 venue 侧成交可能未送达，重放不覆盖该缺口，靠对账补齐
