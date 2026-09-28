# 架构总览（Architecture Overview）

> 状态：基线 · 真相源：docs/concepts/architecture.md、docs/developer_guide/design_principles.md、crates 结构
> 本文档是 OpenLogos 技术架构层唯一入口；详细概念文档位于 docs/concepts/

## 1. 架构风格

NautilusTrader 采用以下架构技术与模式（来源：docs/concepts/architecture.md）：

- **领域驱动设计（DDD）**：订单/仓位/事件/工具等领域模型集中（crates/model）
- **事件驱动架构**：一切状态变化皆事件，经 MessageBus 流转
- **消息模式**：发布/订阅、请求/响应、点对点三模式并存
- **端口与适配器（六边形架构）**：venue 接入统一为 DataClient/ExecutionClient 端口
- **Crash-only 设计**：无优雅停机特殊路径，恢复即事件重放

## 2. 系统边界与组件图

系统边界 = 一个 Nautilus 节点实例的运行时。

```
Data clients ──market data──▶ DataEngine ──store──▶ Cache
                                 │ publish
                                 ▼
Exec clients ◀──▶ ExecutionEngine    MessageBus ──callbacks──▶ Trader
                   │                    │                        │ actors, strategies,
                   │ execution state    │ order/position events  │ algorithms
                   ▼                    ▼                        │
                 Cache ◀──read── RiskEngine ◀──trading commands──┘
                             │ read portfolio
                             ▼
                         Portfolio ◀──account/order/position/price events── MessageBus
```

**NautilusKernel 六组件**：DataEngine、RiskEngine、ExecutionEngine、Portfolio、Trader（actors/strategies/algorithms）、MessageBus + Cache。

## 3. 组件职责

| 组件 | crate | 职责 |
|---|---|---|
| DataEngine | data | 行情订阅/请求/发布、历史数据回放、自定义数据注册 |
| RiskEngine | risk | TradingCommand 预检（额度/价格/频率），读 Cache 与 Portfolio 状态 |
| ExecutionEngine | execution | 命令下发 venue 适配器；执行事件回写 Cache、上总线 |
| Portfolio | portfolio | 消费账户/订单/仓位/价格事件，增量核算盈亏与敞口 |
| Trader | trading | Actor/Strategy/Algorithm 容器与生命周期 |
| MessageBus | common | 进程内消息中枢，三消息模式，可外接 backing |
| Cache | common/model | 状态存储（订单/仓位/账户/工具），可外接 backing |

## 4. 环境上下文（Environment Contexts）

同一内核支撑两类环境（FR-002 的架构兑现）：

- **BacktestEngine**（crates/backtest）：确定性仿真，SimulatedExchange 模拟撮合与流动性
- **LiveExecEngine**（crates/live）：真实 venue 客户端、时钟、心跳、重连

## 5. 持久化与事件溯源

- 领域事件全量记录（crates/event_store），可序列化（crates/serialization：msgpack 等，catalog parquet）
- 恢复 = 事件重放 + 可选 Cache/Bus backing（FR-004/FR-005 的架构兑现）
- **无自有数据库**：logos/resources/database/ 保持为空是有意决策，非缺失

## 6. 进程与部署形态

- 单节点进程：一个 Python 或 Rust 进程内完整 Kernel
- 无服务化拆分：引擎不是微服务集群，扩展靠多节点进程（用户自管）
- 详细部署模型见 `../3-deployment/01-deployment-model.md`

## 7. 跨切关注

- 日志：结构化，经 LoggingConfig（docs/concepts/logging.md）
- 序列化：serialization crate 统一格式
- 网络：network crate 提供 websocket/REST 基座
- 插件：plugin crate 支持运行时扩展
