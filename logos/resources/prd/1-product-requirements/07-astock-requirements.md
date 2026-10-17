# A股数据接入需求（A-Stock Data Requirements）

> 变更提案：a-stock-data-integration · 分支：a-stock · 状态：基线

## 背景

18 个既有适配器零中国内地覆盖。A股接入首期聚焦**日线数据**（低频场景：选股/多因子/轮动，
详见 01-overview 与用户故事），执行通道（CTP/XTP/QMT 柜台）不在本提案范围。

## 功能性需求

| ID | 需求 | 优先级 |
|---|---|---|
| FR-A1 | 双源日线：通达信静态文件 + TickFlow API，共同维护全量 A股日线 | P0 |
| FR-A2 | 通达信源：解析 TDX `.day` 二进制日线（每股一文件，32 字节/记录），支持人工下载后批量导入 | P0 |
| FR-A3 | TickFlow 源：`GET /v1/klines`（symbol/period=1d/count≤10000/start_time/end_time/adjust），`x-api-key` 认证 | P0 |
| FR-A4 | 合并策略：以 catalog 为汇聚点，同标的按 `ts_event` 去重；冲突时 TickFlow（在线、含除权）优先于通达信静态导出 | P0 |
| FR-A5 | 统一输出：Nautilus `Bar`（如 `600000.SH-1-DAY-LAST-EXTERNAL`）+ `Equity` Instrument（精度 0.01，lot 100） | P0 |
| FR-A6 | 复权处理：TickFlow `adjust` 参数透传（qfq/hfq/none）；通达信源默认不复权，由 FR-A4 冲突规则收敛 | P1 |
| FR-A7 | 覆盖范围：沪深主板/创业板/科创板股票 + ETF（TickFlow 文档声明支持） | P1 |
| FR-A8 | 增量更新：TickFlow 按 `start_time` 拉增量，避免全量重拉 | P1 |

## 非功能性需求

| ID | 需求 |
|---|---|
| NFR-A1 | API Key 仅经环境变量 `TICKFLOW_API_KEY` 注入，不入库不入日志（沿用 NFR-009） |
| NFR-A2 | TickFlow 限速礼貌重试（指数退避），失败不阻塞通达信源加载 |
| NFR-A3 | `.day` 解析零第三方依赖（struct 解包），单文件 10 万行解析 < 1s |
| NFR-A4 | 合并幂等：重复运行同一数据集不产生重复 Bar |

## 用户故事（补充）

**US-A1** 作为量化研究员，我想把通达信里人工下载的日线一键导入 Nautilus catalog，
以便离线完成深度历史回测（数据早已在手，不依赖网络）。

**US-A2** 作为量化研究员，我想让 TickFlow 每日自动补齐最新日线与除权因子，
以便 catalog 始终可用而不必天天手动导文件。

## 边界（本期不做）

- 分钟线/tick（TickFlow 已有 1m~60m 能力，架构预留 period 扩展点即可）
- 实时行情订阅（五档/实时接口在 TickFlow 存在，属 DataClient 实盘范畴）
- 下单执行通道（ExecutionClient：CTP/XTP/QMT）

## FR-A9 ST 风险筛选器

### FR-A9 需求

| ID | 需求 | 优先级 |
|---|---|---|
| FR-A9 | A股 ST 风险研究筛选器：基于可量化财务与行情指标对股票池进行 `hard`/`warn`/`clean` 三级研究风险评分；`hard` 用于保守排除，`warn` 提示人工复核，`clean` 表示未触发可量化判据；输出**不得替代**交易所正式 *ST/ST 公告 | P1 |

### 可观察验收条件

| AC ID | 验收条件 | 覆盖用例 |
|---|---|---|
| S10-AC-05 | 财务与行情研究判据、健康样本、无数据保守排除的输出与当前实现一致；通过具体 reasons 区分研究命中与数据不足 | UT-STF-01, UT-STF-02, UT-STF-03, UT-STF-04, UT-STF-05, UT-STF-06, UT-STF-07, UT-STF-08 |
| S10-AC-06 | 年报/季报混合输入实际按最近 N 期报告判定；测试揭示现有“连续N年”原因文本与季度输入不符，文档明确该局限，不宣称已修复年度判定 | UT-STF-09 |
| S10-AC-07 | 实际解析正数和缺失字段，断言缺失数值为 None、无公告/修订字段且可能输出 clean、空 reasons；文档明确 clean 不能证明财务健康 | UT-STF-10 |
| S10-AC-08 | `screen_pool` → `apply_pool` 内部调用链对 hard/warn/clean 分类正确；默认保留 clean/warn，drop_warn 时仅保留 clean | ST-STF-01 |
| S10-AC-09 | pytest 真实子进程中的成功、call失败、setup失败、skip、teardown失败分别生成正确 JSONL；失败带非空 error，不被成功 teardown 覆盖 | UT-STF-11 |

### 研究假设阈值与当前配置行为

| 判据 | 默认值 | 说明 |
|---|---|---|
| 每股净资产 < 0 | `FinancialMetrics.negative_equity` | 实际硬编码比较 bps<0；配置字段 min_bps 当前未被读取，不能声称调整该字段会生效 |
| 最近 N 期均净亏损 | `consecutive_loss_years = 2` | 当前实现按传入报告列表最后 N 期判定，**不保证会计年度连续** |
| 单期净亏损 | 最新报告净利润<0 | 未命中其他 hard 条件时返回 warn；配置字段 loss_warn_years 当前未被读取 |
| 亏损且营收 < 1 亿元 | `min_revenue_cny = 1.0e8` | 对应规则 1；`revenue` 缺失时退化为仅亏损 `warn` |
| 最近可用收盘价均值 < 2 元 | `min_price = 2.0` | 默认取最后最多60条输入；不足60条也计算，不保证完整交易日窗口 |
| 最近可用成交额均值 < 500 万元 | `min_avg_turnover_cny = 5.0e6` | 默认取最后最多60条成交额；未提供成交额时跳过此判据，不生成缺失提示 |

> 上述阈值仅为研究起点，不预设赚钱；涉及正式监管阈值须后续核对交易所规则版本、适用板块和生效日期。

### 当前数据边界

- 当前接口**没有 `unknown` 枚举**。直接调用 judge_financials 的空列表返回 hard+no-financial-data；直接调用 judge_market 的空收盘列表返回 hard+no-market-data。screen_pool 对缺失财务键和空行情跳过相应判据；两源均不可用才返回 hard+no-data。单源存在或部分字段缺失时仍可能 clean、空 reasons，不能认定财务健康。
- `FinancialMetrics` 不含 `available_at`、`revision`、`source_hash`，无法按历史 `as_of` 重算，也无法区分修订值与原历史版本。
- 筛选器为**独立 Python 辅助模块**，不接入 S12 Rust 规则引擎，也不进入 Rust 快照 auxiliary 通道。
- 本能力**禁止用于严格历史研究**；在正式 *ST/ST 状态核对、审计意见覆盖、非量化触发条件场景下不具备权威性。

### 用户故事（补充）

**US-A3** 作为量化研究员，我想在导入 A股日线后用一个独立 Python 工具对股票池做 ST 风险预筛查，以便快速获得可量化的风险提示并知道哪些标的因数据不足被保守排除。

### 边界（本期不做）

- 接入交易所正式 *ST/ST 公告数据；
- 覆盖审计意见与非量化触发条件；
- 按历史 as_of 重算、会计年度连续亏损、修订版本冻结；
- 与 S12 Rust 规则引擎或 Rust 快照 auxiliary 通道集成。
