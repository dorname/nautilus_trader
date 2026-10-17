## ADDED — ST 风险筛选器测试用例

### 一、单元测试用例

| 用例 ID | 关联需求 | 用例描述 | 前置/步骤 | 期望结果 |
|---|---|---|---|---|
| UT-STF-01 | FR-A9 | 净资产为负返回 `hard` | `metrics=[FinancialMetrics(period_end="2025-12-31", net_profit=1e6, bps=-0.5)]` | `severity=="hard"`、`excluded==True`、`reasons` 含 BPS<0 |
| UT-STF-02 | FR-A9 | 最近 N 期均净亏损返回 `hard`（当前非会计年度） | `metrics=[2024年报 np=-5e7, 2025年报 np=-3e7]` | `severity=="hard"`、`excluded==True`、`reasons` 含连续/最近 N 期亏损 |
| UT-STF-03 | FR-A9 | 单期亏损返回 `warn` | `metrics=[2024年报 np=5e7, 2025年报 np=-2e7]` | `severity=="warn"`、`excluded==False` |
| UT-STF-04 | FR-A9 | 健康标的返回 `clean` | `metrics=[2024年报 np=8e8 bps=5.0, 2025年报 np=9e8 bps=5.5]` | `severity=="clean"`、`excluded==False`、`reasons` 为空 |
| UT-STF-05 | FR-A9 | 亏损且营收低于 1 亿元返回 `hard` | `metrics=[2025年报 np=-1e7 bps=0.8, raw={"revenue":6e7}]` | `severity=="hard"`、`reasons` 含营收<1亿 |
| UT-STF-06 | FR-A9 | 长期均价低于 2 元返回 `hard` | `bars_close=[1.8]*30+[1.9]*30` | `severity=="hard"`、`reasons` 含均价<2元 |
| UT-STF-07 | FR-A9 | 日均成交额低于500万元返回warn | bars_close=[5.0]*100、bars_turnover=[3e6]*100 | severity为warn、excluded为False，reasons含实际文本“日均成交额<5M” |
| UT-STF-08 | FR-A9 | 空输入保守排除与汇总跳过判据的区别 | 分别调用screen_pool无两源、judge_financials空列表、judge_market空列表；另构造仅正常行情的标的 | 前三者分别hard+no-data、hard+no-financial-data、hard+no-market-data；仅正常行情的汇总可clean，不能把不存在的财务判据误记为执行过 |
| UT-STF-09 | FR-A9 | 年报与同年度两份季报均亏损，揭示最近N期不等于连续年度 | 三份报告日期2024-12-31、2025-03-31、2025-06-30，净利润分别-1e8、-5e7、-3e7 | hard且原因实际仍含“连续2年净亏损”；原因仅列2025-03-31与2025-06-30、不列2024年报；测试注释及本规格明确该文本有误，本批未修复年度识别 |
| UT-STF-10 | FR-A9 | 财务解析与缺失字段的实际边界 | _parse分别解析有效symbol/净利润/bps响应和缺失净利润、bps响应；后者送入judge_financials | 有效字段保值；缺失数值为None，FinancialMetrics无available_at/revision属性；缺失数值可能仍输出clean且reasons为空，此结果不能作为财务健康或历史可见性证明 |
| UT-STF-11 | FR-A9 | `conftest.py` 跨 pytest 阶段状态聚合修正 | 真实 pytest 子进程运行：成功用例、call 失败用例、setup 失败用例、skip 用例、teardown 失败用例 | JSONL 中状态优先级 `fail > skip > pass`；失败用例带 `error`；三阶段时长累计；成功/skip 无 `error`；teardown 失败不因 teardown 阶段 `passed` 而误报 pass |

### 二、场景测试用例

| 用例 ID | 描述 | 覆盖 Steps | 前置条件 | 操作序列 | 预期结果 |
|---|---|---|---|---|---|
| ST-STF-01 | `screen_pool` → `apply_pool` 内部 API 调用链 | Step 1→4 | 固定小样本：GOOD（clean）、WARN（单期亏损）、LOSS（连亏 hard）、NODATA（无数据 hard） | 1. 构造 `fin_metrics`/`catalog_bars`；2. 调用 `screen_pool`；3. 调用 `apply_pool(drop_warn=False)`；4. 调用 `apply_pool(drop_warn=True)` | `verdicts[GOOD].severity=="clean"`、`verdicts[WARN].severity=="warn"`、`verdicts[LOSS].severity=="hard"`、`verdicts[NODATA].severity=="hard"`；默认池含 GOOD+WARN；drop_warn 池仅含 GOOD |

### 三、覆盖度校验

- [x] FR-A9 可观察验收条件：全部覆盖（S10-AC-05～09）
- [x] 数据不足保守排除：UT-STF-08
- [x] 年报/季报混合限制：UT-STF-09
- [x] 缺失字段边界：UT-STF-10
- [x] reporter 阶段聚合：UT-STF-11
- [x] 内部 API 编排：ST-STF-01

### 四、验收条件追溯

| AC ID | 验收条件 | 覆盖用例 |
|---|---|---|
| S10-AC-05 | ST 风险筛选器基本判据与保守排除 | UT-STF-01, UT-STF-02, UT-STF-03, UT-STF-04, UT-STF-05, UT-STF-06, UT-STF-07, UT-STF-08 |
| S10-AC-06 | 年报/季报混合输入的非年度限制 | UT-STF-09 |
| S10-AC-07 | 缺失字段不伪造结果 | UT-STF-10 |
| S10-AC-08 | `screen_pool` → `apply_pool` 分类正确 | ST-STF-01 |
| S10-AC-09 | reporter跨阶段状态与真实子进程退出对应 | UT-STF-11 |
