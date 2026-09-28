# 信息架构与功能规格总览（Information Architecture & Feature Specs）

> 状态：基线 · 真相源：crates/ 目录结构、python/nautilus_trader 包结构、docs/concepts
> 定位：本文件是"产品由什么功能块组成"的唯一规格入口，需求（FR）与实现（crate）在此汇合

## 1. 顶层信息架构

产品按三层组织：**内核（Kernel）→ 能力域（Domains）→ 接入面（adapters/语言面）**

```
NautilusTrader
├── 内核 runtime
│   ├── system（NautilusKernel 组装、节点）
│   ├── common（MessageBus、组件生命周期）
│   └── model（领域模型：订单/仓位/事件/工具）
├── 能力域
│   ├── data（行情/历史数据引擎）       ├── risk（风控引擎）
│   ├── execution（执行引擎）           ├── portfolio（组合核算）
│   ├── indicators（指标库）            ├── analysis（绩效分析）
│   ├── backtest（回测引擎）            ├── live（实盘运行时）
│   ├── persistence（持久化/catalog）   ├── serialization（事件序列化）
│   ├── event_store（事件存储）         ├── network（网络客户端基座）
│   └── testkit（测试工具）
├── 接入面
│   ├── adapters（18 个 venue 适配器）
│   ├── pyo3 + python 包（Python 控制面）
│   └── cli / config（命令行与配置）
```

## 2. 能力域 ↔ crate ↔ 需求 追踪矩阵

| 能力域 | Rust crate | Python 包 | 承载需求 |
|---|---|---|---|
| 内核组装/节点 | system | system, config | FR-002, FR-010 |
| 消息总线/组件 | common | common | FR-011 |
| 领域模型 | model | model | FR-001, FR-003 |
| 数据引擎 | data | data | FR-014, FR-015 |
| 风控 | risk | risk | FR-008 |
| 执行 | execution | execution | FR-003, FR-004 |
| 组合核算 | portfolio | portfolio | FR-009 |
| 指标 | indicators | indicators | FR-006 |
| 绩效分析 | analysis | analysis | FR-012 |
| 回测 | backtest | backtest | FR-002, FR-017 |
| 实盘 | live | live | FR-010 |
| 持久化 | persistence | persistence | FR-004, FR-005 |
| 序列化 | serialization | serialization | FR-004 |
| 事件存储 | event_store | — | FR-004 |
| 网络 | network | network | FR-010, FR-013 |
| 测试工具 | testkit | testkit | FR-016 |
| 适配器 | adapters/* | adapters/* | FR-001, FR-013, FR-014 |
| Python 桥 | pyo3 | — | FR-007 |
| 加密/工具 | core, cryptography, macros, infrastructure, plugin, trading, serialization | 对应镜像 | 横切 |

## 3. 关键功能规格（规格级描述，非代码）

### 3.1 节点类型（system/config）
- `BacktestNode`：多引擎批量回测编排（研究入口）
- `LiveExecNode`：实盘执行节点（生产入口）
- 两类节点共享 Actor/Strategy 注册、Kernel 组件装配协议

### 3.2 策略模型（trading）
- Strategy 封装订单/持仓/订阅 API；Actor 提供非交易型组件（数据聚合、监控）
- 生命周期钩子：on_start/on_event/on_order_event/on_position_event/on_stop…

### 3.3 引擎组件协议
- DataEngine：订阅/请求/发布行情与历史数据，写入 Cache
- RiskEngine：TradingCommand 预检（额度/价格/频率），读 Cache 与 Portfolio
- ExecutionEngine：命令下发至 venue 适配器，事件回写 Cache 并上总线
- Portfolio：消费账户/订单/仓位/价格事件，增量核算

### 3.4 适配器规格（adapters）
- 每个 venue 提供 DataClient（行情/历史）与 ExecutionClient（订单/账户）实现
- 18 个 venue：architect_ax、betfair、binance、blockchain、bybit、coinbase、databento、deribit、derive、dydx、hyperliquid、interactive_brokers、kraken、lighter、okx、polymarket、sandbox、tardis
- sandbox 适配器作为参考实现模板（P4 画像入口）

### 3.5 配置体系（config / python）
- Python-driven 配置对象（TradingNodeConfig、BacktestRunConfig 等），可序列化为 JSON
- .env 凭证注入（NFR-009）

## 4. 页面/交互设计

本产品无 GUI（设计决策记录：`../2-page-design/01-no-gui-design-decision.md`）。
用户交互面 = Python API + Rust API + CLI + 日志/报告文件输出。
