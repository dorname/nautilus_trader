# 实现清单基线（Implementation Inventory）

> 状态：基线 · 真相源：crates/、python/nautilus_trader/ 目录扫描（2026-09-28）
> 用途：需求 ↔ 实现双向追踪的"实现侧"锚点；后续变更提案在此清单上做增删

## 1. Rust workspace（26 个 crate）

| # | crate | 域 | 实现 | 支撑需求 |
|---|---|---|---|---|
| 1 | core | 基础 | 核心原语 | 横切 |
| 2 | common | 基础 | MessageBus、Component 生命周期 | FR-011 |
| 3 | model | 领域 | 订单/仓位/事件/工具模型 | FR-001、FR-003 |
| 4 | system | 内核 | NautilusKernel、节点 | FR-002、FR-010 |
| 5 | data | 引擎 | DataEngine、catalog、wrangler | FR-014、FR-015 |
| 6 | risk | 引擎 | RiskEngine | FR-008 |
| 7 | execution | 引擎 | ExecutionEngine | FR-003、FR-004 |
| 8 | portfolio | 引擎 | Portfolio 核算 | FR-009 |
| 9 | trading | 应用 | Strategy/Actor/Algorithm | FR-007 |
| 10 | indicators | 应用 | 技术指标 | FR-006 |
| 11 | analysis | 应用 | 绩效分析 | FR-012 |
| 12 | backtest | 环境 | BacktestEngine、SimulatedExchange | FR-002、FR-017 |
| 13 | live | 环境 | LiveExecEngine 运行时 | FR-010 |
| 14 | network | 接入 | websocket/REST 基座 | FR-010、FR-013 |
| 15 | adapters | 接入 | 18 个 venue 适配器（子目录） | FR-001、FR-013、FR-014 |
| 16 | persistence | 横切 | 持久化/catalog | FR-004、FR-005 |
| 17 | serialization | 横切 | 事件序列化 | FR-004 |
| 18 | event_store | 横切 | 事件存储 | FR-004 |
| 19 | pyo3 | 桥接 | Rust↔Python FFI | FR-007 |
| 20 | testkit | 质量 | 测试工具 | FR-016 |
| 21 | cli | 工具 | 命令行 | 工程效率 |
| 22 | cryptography | 工具 | 加密 | NFR-009 |
| 23 | macros | 工具 | 过程宏 | 工程效率 |
| 24 | infrastructure | 工具 | 基础设施 | 工程效率 |
| 25 | plugin | 扩展 | 插件机制 | 可扩展性 |
| 26 | analysis→(see 11) / serialization(见 17) | — | （去重占位，实际清单以 crates/ 为准） | — |

## 2. venue 适配器（18）

architect_ax · betfair · binance · blockchain · bybit · coinbase · databento · deribit ·
derive · dydx · hyperliquid · interactive_brokers · kraken · lighter · okx · polymarket ·
sandbox · tardis

分类：加密交易所（binance/bybit/okx/coinbase/deribit/dydx/hyperliquid/kraken/lighter/derive）、
传统金融、数据供应商、预测/事件市场（polymarket/betfair）、区块链、开发模板（sandbox）。

## 3. Python 包镜像

python/nautilus_trader 镜像上述域（model/data/execution/portfolio/…），另含：
_libnautilus（PyO3 扩展）、adapters、testkit、config；stubs 由 generate_stubs.py 生成。

## 4. 缺口分析（基线结论）

- **文档层**：OpenLogos 资源目录此前全空 → 本提案（add-baseline-docs）补齐
- **代码层**：基线无新增代码任务（本提案为纯规格变更）
- 后续演进入口：ROADMAP.md（项目路线图）
