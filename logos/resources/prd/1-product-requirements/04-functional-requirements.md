# 功能性需求（Functional Requirements）

> 状态：基线 · 每条需求标注来源（仓库事实），验收以行为为准
> 优先级：P0=核心不可缺，P1=重要，P2=增强

## FR-001 多资产多场所支持（P0）
引擎支持跨资产类别（现货、期货/连续合约、期权、预测市场、组合工具 synthetics）与多场所并发运行。
来源：README"multi-asset, multi-venue"；docs/concepts/instruments/、continuous_futures.md、options.md、synthetics.md。

## FR-002 回测与实盘同构（P0）
BacktestEngine（确定性仿真）与 LiveExecEngine 共享同一 NautilusKernel 组件集（DataEngine/RiskEngine/ExecutionEngine/Portfolio/MessageBus/Cache），策略代码零改动迁移。
来源：docs/concepts/architecture.md、docs/concepts/backtesting/。

## FR-003 统一订单生命周期（P0）
订单状态机覆盖：提交→已接受/拒绝→部分成交→成交→撤销/过期/终止，含订单修改、批量单、算法执行单（algorithmic execution）。
来源：docs/concepts/orders/、crates/execution、crates/model。

## FR-004 事件溯源（P0）
所有状态变化以领域事件记录，可重放重建状态；事件可序列化（parquet 等格式经 crates/serialization、crates/persistence）。
来源：docs/concepts/event_sourcing.md、crates/event_store。

## FR-005 崩溃恢复（P0）
crash-only 设计：节点通过事件重放与可选 cache backing 恢复订单/仓位/账户状态。
来源：docs/concepts/architecture.md"Crash-only design"。

## FR-006 技术指标库（P1）
内置常用技术指标（crates/indicators），策略内可组合使用。
来源：crates/indicators、python/nautilus_trader/indicators。

## FR-007 双语言策略（P0）
策略可用 Python 或纯 Rust 编写；Python 经 PyO3 桥接调用 Rust 内核。
来源：docs/concepts/python.md、docs/concepts/rust.md、crates/pyo3。

## FR-008 集中风控引擎（P0）
RiskEngine 对每条 TradingCommand 预检（额度、价格、频率等），违规拒绝并发布事件；风控状态可查询。
来源：docs/concepts/architecture.md、crates/risk。

## FR-009 组合核算（P0）
Portfolio 增量维护账户、仓位、未实现/已实现盈亏，支持多币种（docs/concepts/accounting.md）。
来源：crates/portfolio。

## FR-010 实盘运行时（P0）
LiveExecNode 提供实盘运行环境：websocket/REST 客户端管理、心跳、重连、时钟同步。
来源：crates/live、crates/network、docs/concepts/live.md。

## FR-011 消息总线（P0）
进程内 MessageBus 支持发布/订阅、请求/响应、点对点消息模式，可外接 backing。
来源：docs/concepts/message_bus.md、crates/common。

## FR-012 绩效分析（P1）
PortfolioAnalyzer 提供绩效统计（收益率、回撤等，crates/analysis），回测产出报告（docs/concepts/reports.md）。
来源：crates/analysis、python/nautilus_trader/analysis。

## FR-013 适配器框架（P0）
端口适配器架构统一 DataClient/ExecutionClient 抽象；内置 18 个 venue 适配器（betfair、binance、blockchain、bybit、coinbase、databento、deribit、derive、dydx、hyperliquid、interactive_brokers、kraken、lighter、okx、polymarket、sandbox、tardis、architect_ax）。
来源：crates/adapters/、ADAPTERS.md。

## FR-014 历史数据引擎（P1）
DataEngine 支持历史数据请求/订阅/回放，含 catalog 与 wrangler（crates/data）。
来源：crates/data、docs/concepts/data/。

## FR-015 自定义数据（P1）
用户可定义自定义数据类型注入事件流参与回测与实盘。
来源：docs/concepts/custom_data.md。

## FR-016 测试工具包（P1）
testkit 提供组件回放与仿真测试工具（crates/testkit、python/nautilus_trader/testkit）。
来源：crates/testkit。

## FR-017 行为模型（P2）
支持对手方/流动性行为模型用于更真实的仿真（behavioral models）。
来源：docs/concepts/behavioral_models.md。

## FR-018 可视化辅助（P2）
提供可视化输出辅助（docs/concepts/visualization.md）。
来源：docs/concepts/visualization.md。
