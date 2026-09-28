# 竞品格局（Competitor Landscape）

> 状态：基线 · 定性对比，聚焦开源量化交易引擎/框架赛道

## 对比维度

开源许可 / 语言栈 / 回测-实盘同构 / 多场所适配 / 事件溯源与恢复 / 生产级定位

## 主要竞品

| 产品 | 许可 | 栈 | 回测-实盘同构 | 多场所 | 特点 | 相对 Nautilus 的差异 |
|---|---|---|---|---|---|---|
| **Backtrader** | GPL-3.0 | Python | 部分（需自行接 broker） | 少量社区插件 | 简单易上手，社区老牌 | 纯 Python 性能受限；项目维护趋缓；GPL 更严格 |
| **Zipline (Reloaded)** | Apache-2.0 | Python | 否（偏研究回测） | 无实盘 | Quantopian 血统，因子研究友好 | 无实盘执行路径，无风控/组合引擎 |
| **QuantConnect Lean** | Apache-2.0 | C#/Python | 是 | 多场所（内置较多） | 全托管云+本地开源引擎 | C# 内核生态较封闭；Python 体验经桥接；体积大 |
| **Freqtrade** | GPL-3.0 | Python | 是 | 加密为主 | 加密散户生态、带 Telegram UI | 场景聚焦加密现货/合约；非通用多资产引擎 |
| **Jesse** | MIT（部分商用） | Python | 是 | 加密为主 | 研究体验好 | 同上，场所与资产类别覆盖窄 |
| **Hikyuu / VN.Py** | MIT | C++/Python | 是（VN.Py） | 国内期货为主 | 中文社区强 | 场所以国内市场为中心；架构非事件溯源 |
| **自研 C++/内部系统** | 私有 | C++ | 是 | 内部 | 性能极致 | 成本高、人才稀缺；Nautilus 提供开源替代路径 |

## NautilusTrader 差异化定位

1. **Rust 数据面 + Python 控制面**：兼顾性能与灵活性，且支持纯 Rust 策略（多数竞品二选一）。
2. **事件溯源 + crash-only**：审计与恢复能力是同类开源品中少有的生产级设计。
3. **18 个 venue 适配器横跨加密、传统金融（IB）、数据供应商（Databento/Tardis）、预测市场（Polymarket/Betfair）**：资产广度领先。
4. **LGPL-3.0**：比 GPL 竞品对商业集成更友好（动态链接不受传染）。
5. **确定性回测 + 同构实盘**：直接针对"回测实盘割裂"痛点。

## 竞争劣势（如实记录）

- 学习曲线陡：Rust + 事件驱动 + DDD 概念门槛高于 Backtrader/Freqtrade
- 无 GUI，对非程序员不友好（设计决策见 2-page-design/01-no-gui-design-decision.md）
- 文档以英文为主；社区规模小于 Lean/Backtrader
