# 场景实现映射（Scenario Implementation Map）

> 状态：基线 · 变更提案：complete-other-specs
> 本目录（2-scenario-implementation）承载"场景 → 实现路径"映射；场景本身的编排规格位于
> `logos/resources/scenario/st-0*.json`，用例规格位于 `logos/resources/test/scenario/scenario-test-cases.md`

## 场景实现索引

| 场景 | 旅程 | 关键实现组件（crate） | 编排规格 |
|---|---|---|---|
| ST-01 | 首次回测 | system(BacktestNode) → data → risk → execution → backtest(SimulatedExchange) → portfolio → analysis | scenario/st-01.json |
| ST-02 | 多场所组合 | adapters/*(两场所) → data → execution → portfolio(多场所汇总) | scenario/st-02.json |
| ST-03 | 风控拦截 | trading(Strategy) → risk(拒单) → common(事件总线) | scenario/st-03.json |
| ST-04 | 回测转实盘零改动 | 同一 Strategy 经 system(BacktestNode/LiveExecNode) + adapters/sandbox | scenario/st-04.json |
| ST-05 | 崩溃恢复 | event_store + serialization(重放) → cache(状态恢复) | scenario/st-05.json |
| ST-06 | 历史数据研究 | adapters/databento|tardis → data(catalog 回放) → backtest | scenario/st-06.json |
| ST-07 | 自定义数据融合 | data(自定义类型注册) → trading(消费) | scenario/st-07.json |
| ST-08 | 纯 Rust 节点 | system/data/risk/execution/portfolio 直接以 Rust API 组装（不经 pyo3） | scenario/st-08.json |

## 与需求追踪

ST-01~08 ↔ US-001~US-010 ↔ FR-001~FR-018 的对应关系见
`prd/1-product-requirements/03-user-stories.md` 与各 JSON 的 `requirements` 字段。
