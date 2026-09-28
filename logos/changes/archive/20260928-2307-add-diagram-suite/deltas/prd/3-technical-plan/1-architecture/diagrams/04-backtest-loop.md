# D-04 · 回测主循环（处理流程图）

> 真相源：docs/concepts/backtesting/execution-flow.md —— 官方"每数据点三阶段"循环

```mermaid
flowchart TB
    START(["BacktestEngine 启动<br/>载入数据集与配置"]) --> NEXT["取下一数据点 ts=T<br/>（quote/bar/tick…）"]
    NEXT --> P1["① 交易所处理数据<br/>SimulatedExchange 更新订单簿<br/>MatchingEngine.iterate() 撮合存量订单"]
    P1 --> P2["② 策略接收数据<br/>DataEngine 分发 on_quote/on_bar<br/>策略可提交/撤销/修改订单（入队）"]
    P2 --> P3["③ 结算场所<br/>drain 全部排队命令 → 撮合新订单"]
    P3 --> CASCADE{"撮合成交触发了<br/>新命令？（如 on_order_filled<br/>里的对冲单）"}
    CASCADE -- 是 --> P3
    CASCADE -- 否 --> SIM["运行仿真模块<br/>（FillModel·延迟·行为模型）"]
    SIM --> MORE{"还有数据点？"}
    MORE -- 是 --> NEXT
    MORE -- 否 --> REPORT["产出绩效报告<br/>（PortfolioAnalyzer）"]
    REPORT --> DONE(["回测结束（确定性输出）"])
```

**关键不变量**
- **同一时间戳内级联结算**：对冲链在同 T 内全部撮合完成，不留跨时间戳悬单
- **数据先于回调**：市场状态更新 → 才轮到策略——避免策略看到过期簿
- **确定性**：无墙钟、无随机源（除非显式种子），同输入必同输出（NFR-003）
