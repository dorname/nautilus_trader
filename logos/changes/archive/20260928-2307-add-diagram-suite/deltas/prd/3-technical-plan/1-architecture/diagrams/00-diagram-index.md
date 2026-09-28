# 图集总览（Diagram Suite Index）

> 变更提案：add-diagram-suite · 总-分结构：1 总图 + 3 架构分图 + 3 流程图 + 3 时序图
> 载体：Mermaid（GitHub/文档站原生渲染）· 真相源：docs/concepts/*

## 阅读顺序（总 → 分）

| 层 | 图 | 文件 | 回答的问题 |
|---|---|---|---|
| 总 | D-01 系统上下文 | 01-system-context.md | 引擎与谁交互？边界在哪？ |
| 架构 | D-02 内核六组件 | 02-kernel-components.md | 内核由什么组成、怎么连接？ |
| 架构 | D-03 端口与适配器 | 03-adapter-architecture.md | 18 个 venue 如何统一接入？ |
| 流程 | D-04 回测主循环 | 04-backtest-loop.md | 一个数据点如何被处理（三阶段）？ |
| 流程 | D-05 实盘消息处理 | 05-live-message-flow.md | 实盘行情→订单→回报怎么走？ |
| 流程 | D-06 崩溃恢复 | 06-crash-recovery.md | 崩溃后状态如何重建？ |
| 时序 | D-07 订单全生命周期 | 07-seq-order-lifecycle.md | 一笔订单的完整事件时间线？ |
| 时序 | D-08 回测逐数据点 | 08-seq-backtest-tick.md | tick 到成交的精确消息顺序？ |
| 时序 | D-09 实盘对账 | 09-seq-live-reconciliation.md | 引擎与 venue 状态如何对齐？ |

## 使用约定

- 图中组件名 = crate/组件实名（system/common/model/data/risk/execution/portfolio/trading）
- 事件名 = 领域事件实名（OrderSubmitted/OrderAccepted/OrderFilled/…）
- 修改任何图必须走变更提案（图是规格的一部分）
