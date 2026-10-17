# 合并指令

## 变更提案
- 提案名称：astock-st-risk-filter
- 提案目录：logos/changes/astock-st-risk-filter/

## 提案内容

# 变更提案：A股 ST 风险筛选器纳入 OpenLogos 规格与验收（修订版）

## 变更原因

当前分支 `a-stock` 已存在 `python/nautilus_trader/adapters/astock/st_filter.py` 与 `financials_client.py` 的实现，以及 `python/tests/adapters/astock/test_st_filter.py` 共 10 项测试，但该功能尚未写入需求文档、测试用例规格，也未接入 `openlogos verify` 的 OpenLogos reporter。本提案选择**现状收编 + 测试 reporter 接入**：在需求、测试规格中明确当前筛选器的能力边界与限制，不改筛选业务逻辑。

本次修订必须严格区分三类输出，避免用户把研究风险评分误当作交易所正式风险警示状态：

1. **实际风险警示状态**：上交所/深交所依法实施的 *ST（退市风险警示）、ST（其他风险警示）必须以交易所公告为准，依赖审计意见、公告时点、规则版本、非量化触发条件等，本筛选器不具备该权威性。
2. **研究风险评分**：基于可量化财务与行情指标，对股票池给出 `hard`/`warn`/`clean` 三级研究层判定，用于后续股票池规则或人工复核。
3. **数据不足**：当前接口**没有 `unknown` 枚举**；无财务/行情数据时返回 `hard` 并附带 `no-data`/`no-financial-data` 原因，属于**保守排除**，不是命中财务风险；部分字段缺失时仍可能判为 `clean`。这些限制必须披露，且**禁止用于严格历史研究**。

## 变更类型

设计级变更（新增功能需求 + 测试规格 + 测试 reporter 接入；代码主体已存在，**不改筛选业务逻辑**）。

## 变更范围

- 影响的需求文档：
  - `logos/resources/prd/1-product-requirements/07-astock-requirements.md`（新增 FR-A9 ST 风险过滤需求，并明确当前实现边界：无 `unknown` 枚举、无 `available_at`/`revision`、不保证连续会计年度）
- 影响的功能规格：
  - `logos/resources/prd/2-product-design/1-feature-specs/core-02-research-workflow.md`（在“筛选语义”章节明确 `hard`/`warn`/`clean` 为研究风险评分，与交易所 *ST/ST 正式状态区分，声明不覆盖审计意见与非量化触发）
- 影响的业务场景：无新场景编号；本筛选器是**独立 Python 辅助模块**，不接入 S12 Rust 规则引擎，也不进入 Rust 快照 auxiliary 通道
- 影响的部署方案：无
- 影响的 API：内部 Python API（`AstockSTFilter.judge_financials`、`judge_market`、`screen_pool`、`apply_pool`）；未暴露为 HTTP 服务，也不被 S12/S13 调用
- 影响的编排测试：
  - `deltas/scenario/core-st-risk-orchestration.json`（新增 ST-STF-01 内部 API 调用链规格：screen_pool → apply_pool，沿用 A股 S10 辅助子流程，不新增场景编号）
- 影响的 DB 表：无
- 影响的 smoke 测试：无
- 影响的测试用例：
  - `logos/resources/test/astock/astock-test-cases.md`（新增 UT-STF-01~UT-STF-11 与 ST-STF-01 共 12 条用例）
- 影响的测试代码：
  - `python/tests/adapters/astock/test_st_filter.py`（函数命名对齐 OpenLogos 用例 ID，并补充年报/季报混合、缺失字段边界断言）
  - `python/tests/adapters/astock/conftest.py`（修正跨 pytest 阶段状态聚合：setup/call/teardown 取最差状态，避免 teardown 成功掩盖调用/setup 失败；新增 UT-STF-11 真实子进程回归覆盖成功/调用失败/setup 失败/skip/teardown 失败与 JSONL 状态逐项对应）

## 部署影响

- 是否需要部署：否
- 部署原因：本变更为本地库功能与测试规格补充，不涉及服务部署
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

在 A股数据接入模块中新增 ST 风险筛选器（`AstockSTFilter`）的正式规格与验收。本提案**不改筛选业务逻辑**，仅收编现有实现并接入 OpenLogos reporter。

### 当前实现边界（必须在本提案中诚实声明）

- `st_filter.py:65-74` 构造器仅保存 `_fin` 引用，`screen_pool` 完全使用调用者传入的 `fin_metrics`，**不存在自动抓取或财务源到 Rust 快照的生产链**。
- `st_filter.py:83-84` 无财务数据时返回 `hard` 并附带 `no-financial-data` 原因，这是**数据不足的保守排除**，不是命中财务风险。
- `st_filter.py:94-103` 的“连续亏损”取传入 `metrics` 列表的最后 `consecutive_loss_years` 期，按**报告期出现顺序**而非会计年度；若传入年报与季报混合数据，可能把跨报告期的亏损误判为“连续年度亏损”。
- “亏损且营收低于 1 亿元”规则在 `revenue` 缺失但已知亏损时仅给出 `warn`；净利润、净资产等字段缺失时可能判为 `clean`，不能据此认定财务健康。
- `financials_client.py:26-48` 的 `FinancialMetrics` 不含 `available_at`、`revision`、`source_hash` 等时点与溯源字段，因此当前实现**无法按历史 as_of 重算**，也无法区分修订值与原历史版本。
- 综上，当前 `AstockSTFilter` 只能作为**当前时点研究层预筛查工具**；在严格历史回测、正式 *ST/ST 状态核对或历史时点财务评估场景下**禁止使用**。

### 本提案将产出的规格与测试

1. 在 `07-astock-requirements.md` 新增 FR-A9：
   - 明确本筛选器输出为“研究风险评分”，**不得替代**交易所正式 *ST/ST 公告；
   - 声明可量化判据覆盖范围（净资产为负、最近 N 期均净亏损、单期亏损、亏损且营收低于 1 亿元、长期均价低于 2 元、日均成交额低于 500 万元）；
   - 声明不覆盖审计意见、非量化触发条件、历史 as_of 重算、会计年度连续亏损、修订版本冻结；
   - 声明当前接口无 `unknown` 枚举；无数据时返回 `hard`+`no-data`/`no-financial-data` 属于保守排除，不是命中财务风险；
   - 声明本能力**禁止用于严格历史研究**。
2. 在 `core-02-research-workflow.md` 的筛选语义章节补充说明：
   - `hard` 表示研究规则命中或数据不足导致的保守排除，必须通过 `reasons` 区分原因；不表示交易所已实施风险警示；
   - `warn` 表示存在预警信号，需人工复核；
   - 数据不足时保守排除并给出原因，禁止用默认值伪造。
3. 在 `astock-test-cases.md` 新增 12 条用例，具体映射如下：

| 用例 ID | 类型 | 覆盖内容 |
|---|---|---|
| UT-STF-01 | 单元 | 净资产为负返回 `hard` |
| UT-STF-02 | 单元 | 最近 N 期均净亏损返回 `hard`（静态记录：当前非会计年度） |
| UT-STF-03 | 单元 | 单期亏损返回 `warn` |
| UT-STF-04 | 单元 | 健康标的返回 `clean` |
| UT-STF-05 | 单元 | 亏损且营收低于 1 亿元返回 `hard` |
| UT-STF-06 | 单元 | 长期均价低于 2 元返回 `hard` |
| UT-STF-07 | 单元 | 日均成交额低于 500 万元返回 `warn` |
| UT-STF-08 | 单元 | 无任何数据时返回 `hard`+`no-data`（保守排除） |
| UT-STF-09 | 单元 | 构造年报+季报混合亏损输入，验证当前按最近 N 期返回 `hard` 并记录限制 |
| UT-STF-10 | 单元 | 解析缺失字段，验证 `None`、缺少 `available_at`/`revision` 的现状边界 |
| UT-STF-11 | 单元 | `conftest.py` 跨 pytest 阶段状态聚合修正：setup/call/teardown 失败、skip、teardown 失败均映射为正确 JSONL 状态；真实子进程回归 |
| ST-STF-01 | 编排 | 端到端 `screen_pool` → `apply_pool` 内部 API 调用链，覆盖 hard/warn/clean 汇总 |

4. 将现有 `test_st_filter.py` 的测试函数命名对齐用例 ID，以便 `conftest.py` 的 OpenLogos reporter 正确输出 `UT-STF-*` / `ST-STF-*` 结果到 `logos/resources/verify/test-results.jsonl`。
5. 修正 `conftest.py` 的 reporter 逻辑（`pytest_runtest_makereport`）：当前实现仅在 teardown 阶段依据 `report.passed` 写结果，当 call/setup 失败而 teardown 成功时会被误报为 `pass`，call skip 也可能被误报为 `pass`。修正后按 setup/call/teardown 三阶段聚合最差状态，真实子进程回归验证成功/调用失败/setup 失败/skip/teardown 失败与 JSONL 状态逐项对应。

### 范围外待确认项（本次不实现，需在后续变更中由外环审查并批准）

- 接入交易所正式 *ST/ST 公告数据，实现“实际风险警示状态”核对；
- 覆盖审计意见（无法表示意见/否定意见）与非量化触发条件；
- 让 ST 筛选器直接读取 Rust 快照中的 `financial` auxiliary（统一数据溯源），而非外部传入 `fin_metrics`；
- 支持按历史 as_of 重算、会计年度连续亏损与修订版本冻结；
- 若需修正“连续 N 年净亏损”等可能误导的文本，需单独列出最小源码影响（`st_filter.py` 原因字符串 + 对应测试断言）并获外环批准。

以上范围外项如需要纳入本提案，须提交具体扩大范围方案，由外环审查并批准；否则保持本次“现状收编 + 测试 reporter”的边界。

### 历史实现清单注记

批次 7 实现清单中 `test_st_filter.py` “不上报” 是历史事实，本提案**不修改**该历史注记。新接入状态由本提案新增的测试规格与后续 `openlogos verify` 真实验收报告证明，不在本批直接编辑旧规格文件。


## 需要合并的 Delta 文件

### 1. deltas/prd/1-product-requirements/07-astock-requirements.md

- Delta 文件：`logos/changes/astock-st-risk-filter/deltas/prd/1-product-requirements/07-astock-requirements.md`
- 目标目录：`logos/resources/prd/1-product-requirements/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/2-product-design/1-feature-specs/core-02-research-workflow.md

- Delta 文件：`logos/changes/astock-st-risk-filter/deltas/prd/2-product-design/1-feature-specs/core-02-research-workflow.md`
- 目标目录：`logos/resources/prd/2-product-design/1-feature-specs/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/scenario/core-st-risk-orchestration.json

- Delta 文件：`logos/changes/astock-st-risk-filter/deltas/scenario/core-st-risk-orchestration.json`
- 目标目录：`logos/resources/scenario/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/test/astock/astock-test-cases.md

- Delta 文件：`logos/changes/astock-st-risk-filter/deltas/test/astock/astock-test-cases.md`
- 目标目录：`logos/resources/test/astock/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(astock-st-risk-filter): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive astock-st-risk-filter`。
