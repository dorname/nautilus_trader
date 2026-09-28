# D-07 · 时序：订单全生命周期

> 一笔限价单从策略提交到终态的完整消息时间线（回测与实盘同构路径）。

```mermaid
sequenceDiagram
    autonumber
    participant S as Strategy
    participant R as RiskEngine
    participant E as ExecutionEngine
    participant C as ExecutionClient/仿真场所
    participant B as MessageBus
    participant P as Portfolio
    participant CA as Cache

    S->>R: submit_order（TradingCommand）
    R->>CA: 读取最新状态（额度/价格带/频率）
    alt 预检违规
        R-->>B: OrderDenied/Rejected
        B-->>S: on_order_denied/rejected
    else 预检通过
        R->>E: validated command
        E->>C: venue 命令
        C-->>E: OrderSubmitted
        E->>CA: 写入订单状态
        E->>B: 发布 OrderSubmitted
        B-->>S: on_order_submitted
        C-->>E: OrderAccepted
        E->>CA: 更新状态
        E->>B: 发布 OrderAccepted
        B-->>S: on_order_accepted
        loop 部分成交
            C-->>E: OrderPartialFilled（数量·均价）
            E->>CA: 更新成交累计
            E->>B: 发布事件
            B-->>P: 增量核算（仓位/盈亏）
            B-->>S: on_order_partial_fill
        end
        C-->>E: OrderFilled（终态）
        E->>CA: 订单完结
        E->>B: 发布 OrderFilled
        B-->>P: 最终核算
        B-->>S: on_order_filled
    end
```

**终态全集**：Filled · Rejected · Canceled · Expired · Updated（修改后继续）· Denied。
订单类型（12 种）：market · limit · stop_market · stop_limit · market_to_limit ·
market_if_touched · limit_if_touched · trailing_stop_market · trailing_stop_limit · emulated · advanced。
