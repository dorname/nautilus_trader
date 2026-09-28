# 运行时数据流（Runtime Dataflow）

> 状态：基线 · 真相源：docs/concepts/architecture.md 数据流章节、backtesting/live 概念文档

## 1. 回测数据流（BacktestEngine）

```
历史数据(catalog/注入) ─▶ BacktestEngine 时钟推进
    ─▶ DataEngine 发布行情事件 ─▶ MessageBus ─▶ Strategy.on_event/on_quote/tick
    ─▶ Strategy 发出 TradingCommand ─▶ RiskEngine 预检（读 Cache/Portfolio）
    ─▶ ExecutionEngine ─▶ SimulatedExchange 撮合仿真（行为模型可选 FR-017）
    ─▶ 执行事件(OrderFilled 等) ─▶ Cache 状态更新 + MessageBus 发布
    ─▶ Portfolio 消费事件核算 ─▶ PortfolioAnalyzer 绩效统计
    ─▶ 回测报告产出（reports）
```

关键性质：**确定性**——同一数据与配置重放产生相同事件序列（NFR-003）。

## 2. 实盘数据流（LiveExecEngine）

```
venue websocket ─▶ DataClient(适配器) ─▶ DataEngine ─▶ MessageBus ─▶ Strategy
Strategy 命令 ─▶ RiskEngine ─▶ ExecutionEngine ─▶ ExecutionClient(适配器) ─▶ venue REST/WS
venue 回报 ─▶ ExecutionClient 解析 ─▶ ExecutionEngine ─▶ Cache + MessageBus
MessageBus ─▶ Portfolio 核算 / RiskEngine 状态更新 / 外部监控
```

与回测的差异仅在：真实客户端替代仿真撮合、实时时钟替代回放时钟。组件链路完全同构（FR-002）。

## 3. 命令与事件流（核心状态机）

### 3.1 订单命令路径
```
Strategy.submit_order ─▶ OrderInitialized
  ─▶ RiskEngine.evaluate（拒单则 OrderRejected 直接回总线）
  ─▶ ExecutionEngine.execute ─▶ venue/仿真
```

### 3.2 订单事件序列（正向路径）
```
OrderInitialized → OrderSubmitted → OrderAccepted
  → OrderPartialFilled* → OrderFilled
  分支：OrderRejected / OrderCanceled / OrderExpired / OrderUpdated(修改)
```
所有事件写入 Cache 并经 MessageBus 发布；Portfolio 据此维护仓位与盈亏。

### 3.3 恢复路径（crash-only）
```
节点重启 ─▶ 从 event_store/backing 加载历史事件 ─▶ 重放至 Cache/Portfolio
        ─▶ 状态恢复到崩溃前 ─▶ 继续处理新事件
```

## 4. 数据类型流

| 数据类别 | 生产者 | 消费者 |
|---|---|---|
| 行情（Quote/Tick/Bar/Depth） | DataClient / 回放 | DataEngine → 订阅者 |
| 历史数据请求 | Strategy | DataEngine → catalog/venue |
| 领域事件（订单/仓位/账户） | ExecutionEngine/venue | Cache、Portfolio、RiskEngine、Strategy |
| 自定义数据（FR-015） | 用户组件 | DataEngine → 订阅者 |
| 指标流（FR-006） | indicators | Strategy |

## 5. 线程与并发模型

- Rust 侧：tokio 异步任务承载网络 IO；内核消息路径同步低开销分发
- Python 侧：事件回调进入 Python 解释器（经 PyO3）；纯 Rust 路径不经解释器（NFR-001）
