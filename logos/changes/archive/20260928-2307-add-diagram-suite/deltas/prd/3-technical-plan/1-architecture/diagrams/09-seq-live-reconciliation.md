# D-09 · 时序：实盘对账（Reconciliation）

> 真相源：docs/concepts（live/execution 的 reconciliation 概念，官方文档高频主题）。
> 对齐引擎状态与 venue 真实状态，消除漂移。

```mermaid
sequenceDiagram
    autonumber
    participant TRG as 触发源<br/>（启动/重连/周期）
    participant L as LiveExecNode
    participant R as ExecutionEngine
    participant C as ExecutionClient
    participant V as Venue
    participant CA as Cache
    participant B as MessageBus

    TRG->>L: 触发对账
    L->>C: 请求 venue 状态
    C->>V: 查询挂单/成交/账户 REST
    V-->>C: venue 侧订单·成交·余额报表
    C-->>R: 生成对账数据

    par 订单对账
        R->>CA: 读取引擎侧订单状态
        R->>R: 差异计算（引擎 vs venue）
        alt 引擎缺事件（venue 有、引擎无）
            R->>CA: 补写缺失订单/成交
            R->>B: 发布补齐事件流
        else 引擎多事件（venue 无、引擎有）
            R->>R: 生成差异报告
            R->>B: 发布对账告警事件
        else 一致
            R->>B: 无事件（静默通过）
        end
    and 账户对账
        R->>CA: 对齐账户余额/可用额度
    end

    B-->>CA: 状态收敛
    Note over R,V: 对账完成后引擎与 venue 状态一致，<br/>交易继续（或按策略暂停）
```

**触发时机**
- 节点启动/崩溃恢复后（D-06 的收尾步骤）
- WS 重连成功后
- 周期性巡检（可配置）

**对账是对崩溃恢复的必要补充**：事件重放只能重建"引擎已见"的状态，
崩溃窗口内 venue 侧发生的变化必须靠对账拉齐。
