# 产品概述（Product Overview）

> 状态：基线（adopted 逆向整理） · 真相源：README.md、docs/concepts/overview.md、Cargo.toml
> 版本基线：Rust workspace 0.65.0 / 发布线 v2.0.0rc6

## 产品定位

NautilusTrader 是一个开源、生产级、Rust 原生的**多资产、多场所（multi-venue）交易系统引擎**。
产品形态是**引擎 + 框架（库）**，不是 SaaS 服务，也不提供 GUI。

一句话定位：**在单一事件驱动架构内覆盖研究、确定性仿真与实盘执行的高性能交易引擎，
以 Python 为控制面、以 Rust 为数据面。**

## 解决的问题

量化交易系统长期存在"回测与实盘割裂"问题：研究代码（Python）与执行代码（C++/内部系统）
是两套实现，策略从研究迁移到实盘需要重写，行为不一致导致回测结果不可信。

NautilusTrader 用同一套事件驱动内核同时承载回测与实盘：
- 回测是**确定性仿真**（deterministic simulation），事件顺序可复现；
- 实盘执行复用同一套 Engine/Portfolio/Risk 组件；
- 策略代码（Python 或 Rust）在回测与实盘之间**零改动迁移**。

## 核心价值主张

| 价值 | 说明 | 支撑事实 |
|---|---|---|
| 高性能数据面 | Rust 原生内核，编译期安全，无 GIL | 26 个 Rust crates 组成的 workspace |
| 灵活控制面 | Python 负责策略逻辑、配置与编排 | `python/nautilus_trader` 包镜像 Rust 结构，经 PyO3 桥接 |
| 回测=实盘 | 同一内核、同一事件流，确定性回放 | BacktestEngine 与 LiveExecEngine 共享 Kernel 组件 |
| 多资产多场所 | 18 个 venue 适配器：加密、传统金融、预测市场 | `crates/adapters/`：binance、bybit、okx、coinbase、deribit、interactive_brokers、databento、polymarket 等 |
| 可扩展 | 端口适配器架构，自定义数据源/场所可插拔 | `crates/adapters/sandbox`、`crates/plugin`、自定义数据支持（docs/concepts/custom_data.md） |

## 产品边界

**是**：事件驱动交易引擎、回测引擎、实盘执行框架、适配器集合、指标库、测试工具包（testkit）。

**不是**：
- 不是托管服务（用户自行部署运行节点）
- 不是 GUI 交易终端（无界面，纯 API/代码驱动）
- 不内置数据库（持久化通过可选的 cache/message-bus backing 实现）
- 不提供券商/交易所账户（用户自带 venue 接入凭证）

## 目标用户

详见 `02-user-personas.md`。核心三类：量化研究员、量化开发者、自营交易团队。

## 关联文档

- 需求明细：`04-functional-requirements.md`、`05-non-functional-requirements.md`
- 架构如何兑现这些价值：`../3-technical-plan/1-architecture/01-architecture-overview.md`
