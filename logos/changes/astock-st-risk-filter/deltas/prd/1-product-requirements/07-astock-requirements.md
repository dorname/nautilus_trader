## ADDED — FR-A9 ST 风险筛选器

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
