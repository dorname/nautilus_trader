# D-03 · 端口与适配器架构（分图）

> 六边形架构：内核不认识任何具体场所，只认识两个端口。

```mermaid
flowchart TB
    subgraph PORTS["端口（trait 契约）"]
        DP["DataClient 端口<br/>行情·历史数据·订阅"]
        EP["ExecutionClient 端口<br/>订单·账户·回报"]
    end

    subgraph KERNEL["NautilusKernel"]
        DE["DataEngine"] --> DP
        EX["ExecutionEngine"] --> EP
    end

    subgraph ADP["crates/adapters/*（18 个实现）"]
        direction LR
        A1["binance"]
        A2["bybit"]
        A3["okx"]
        A4["coinbase · deribit<br/>kraken · dydx<br/>hyperliquid · lighter<br/>derive"]
        A5["interactive_brokers<br/>（传统金融）"]
        A6["databento · tardis<br/>（数据供应商）"]
        A7["polymarket · betfair<br/>（预测/事件市场）"]
        A8["architect_ax · blockchain"]
        A9["sandbox（参考模板）"]
    end

    DP --> A1 & A2 & A3 & A4 & A5 & A6 & A7 & A8 & A9
    EP --> A1 & A2 & A3 & A4 & A5 & A7 & A8 & A9

    NET["crates/network：WS/REST 基座<br/>心跳·重连·时钟"]
    A1 & A2 & A3 -.-> NET
```

**要点**
- DataEngine 只依赖 `DataClient` trait、ExecutionEngine 只依赖 `ExecutionClient` trait——新增场所零内核改动（FR-013）
- `sandbox` 是官方参考实现：新适配器贡献者（P4）从它起步
- databento/tardis 仅实现 Data 侧（历史数据），不下单——图中 EP 无连线
- 适配器统一走 network 基座获得心跳/重连能力（FR-010）
