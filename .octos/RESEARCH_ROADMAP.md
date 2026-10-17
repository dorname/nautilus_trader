# A股研究后续切片建议

> 标记：`RESEARCH-BOOT-20261017`
> 性质：后续切片建议，**非**已创建或已批准的变更提案；所有证据均为当前仓库静态阅读结论，未经实际运行验证。

---

## 切片 1：研究正确性（运行区间、样本外、信号内核）

### 目标
- 明确运行起止日、预热区间、收益区间与样本外冻结边界；
- 区分固定池形成日与动态池重算；
- 将 EMA/动量/Top-K 模板从当前“单标独立运行”真正接线到组合级一致信号与目标生成。

### 依赖
- S11 数据快照（不可变快照、能力门控）
- S12 股票池（固定/动态成员、时点纪律）
- S13 运行规格（`RunSpec`、`EmaRunConfig`）
- `core-02-research-workflow.md` 策略与成交语义

### 现有证据（静态）
- `crates/research-desktop/src/bridge.rs:240-279`：`submit_run` 构造的 `RunSpec` 中 `training_end`/`test_start` 均为 `None`，当前未实现样本外拆分。
- `crates/research-worker/src/runner.rs:64-83`：`EmaRunConfig` 含 `warmup_bars` 与 `signal_dates`，但 `run_ema_daily` 为单标运行，组合级信号一致性由 `adapter.rs` 在上层拼装。
- `crates/research-worker/src/adapter.rs:277-338`：`run_portfolio` 将总资金按成员数等分槽位，各标独立运行，**未实现组合级共享现金**与跨标一致调仓。
- `crates/research-worker/src/adapter.rs:288-301`：从快照读取的 `bars` 未按 `spec.start`/`spec.end` 裁剪，直接传入引擎。
- `crates/research-worker/src/adapter.rs:217-219`：`default_board` 对所有代码返回 `"主板"`，未按真实板块或规则包逐日解析。
- `crates/research-domain/src/universe.rs:520-534`：`precheck_run_universe` 在生产源码中仅定义，调用仅见 `crates/research-domain/tests/s12.rs:252-260` 单元测试；固定池形成日检查尚未接入运行提交流程。
- `logos/resources/prd/2-product-design/1-feature-specs/core-02-research-workflow.md:15-17`：已规定 EMA/动量模板、调仓日、先卖后买与资金不足行为，但代码层面尚未完全接线。

### 拟影响场景
- S13 研究运行、S14 比较、S15 交易计划。

### 验收缺口
- ST-S13-01 仅验证单标数值，缺少多标组合级 cash-pool 与先卖后买断言；
- 无样本外区间冻结与作弊检查；
- `adapter.rs` 未按运行区间裁剪 bars；
- 默认主板与规则包首日解析可能导致严格模式历史回测失真；
- `precheck_run_universe` 尚未被运行入口调用；
- 动态池在每个调仓日重算规则尚未实现。

### 待外环裁决事项
1. 切片 1 与切片 2 的实施次序：是否先补齐组合级 cash-pool 与先卖后买，再引入样本外评估？
2. 样本外区间是否在本阶段冻结为必需项，还是允许后续独立批次？
3. 动态股票池重算是否优先于默认主板/规则包逐日解析修复？

---

## 切片 2：成交与组合账户（现金共享、规则门、公司行为）

### 目标
- 实现组合级共享现金与“先卖后买”顺序；
- 接入滑点、流动性比例、停牌、涨跌停、按标的与日期加载的规则包；
- 让公司行为（拆股、分红）在组合账本中保持一致，而非仅在估值重放层生效。

### 依赖
- 规则包 auxiliary（`rules`）
- 公司行为 auxiliary（`actions`）
- 交易日历 auxiliary（`calendar`）
- `gate.rs` 规则门、`fees.rs` 费用模型

### 现有证据（静态）
- `crates/research-worker/src/adapter.rs:277-279`：组合按成员数等分资金，**无共享现金**。
- `crates/research-worker/src/runner.rs:224-236`：`on_trade` 已按 T+1 计算当日买入不可卖，但未处理组合级“先卖后买”资金释放。
- `crates/research-worker/src/runner.rs:288-300`：`size_buy` 计算买入量时仅考虑本标槽位现金与费用，未考虑组合其他标的卖出收入。
- `crates/research-worker/src/adapter.rs:239-258`：严格模式按运行区间每一天检查规则包生效区间，但规则按首日解析（`adapter.rs:308-318`），逐日版本化未实现。
- `crates/research-worker/src/adapter.rs:217-219`：`default_board` 固定返回主板，未读取真实板块或规则包。
- `core-02-research-workflow.md:17-20`：已规定滑点、流动性、停牌、涨跌停、费用按生效日期配置，但 `runner.rs` 中未体现滑点/参与率接线。

### 拟影响场景
- S13 研究运行、S14 比较、S15 交易计划。

### 验收缺口
- 无组合级 cash pool 与先卖后买顺序的 UT/ST；
- `RunForm`（`bridge.rs:62-79`）收集了 `slippage_bps`/`participation`，但 `runner.rs` 未使用；
- 流动性比例（单笔不超过前 20 日平均成交量比例）未接线；
- 公司行为仅在末段估值重放生效，引擎持仓未调整（`runner.rs:14-15`、`adapter.rs` 已知限制）。

### 待外环裁决事项
1. 组合级共享现金与先卖后买是否作为 S13 下一批次的最高优先级？
2. 滑点/流动性比例是否本期接入，还是作为后续 fidelity 批次？
3. 公司行为在引擎层生效需要引擎接口支持，是否继续接受“估值层生效 + 限制披露”方案？

---

## 切片 3：指标与人工计划（几何年化、回撤、换手、计划一致性）

### 目标
- 将指标计算对齐 `core-02-research-workflow.md`：几何年化、含初始本金的最大回撤、以每日平均净资产为分母的换手；
- 让交易计划复用回测的信号/目标内核；
- 修复当前计划生成可能超过 100% 总权重的问题，并显式考虑现金、可卖数量与费用约束。

### 依赖
- `crates/research-domain/src/metrics.rs`
- `crates/research-domain/src/plan.rs`
- `crates/research-domain/src/coordinator.rs` 的 `run_plan_compute`

### 现有证据（静态）
- `crates/research-domain/src/metrics.rs:74-89`：`max_drawdown` 初始 `peak = 0`，未把初始本金作为起点，导致首段回撤可能漏算；与 `core-02-research-workflow.md:25` 的“峰值含初始资金”不一致。
- `crates/research-domain/src/metrics.rs:141-150`：年化收益为线性近似（总收益 × 252 / 样本数），与 `core-02-research-workflow.md:25` 要求的几何年化不一致。
- `crates/research-domain/src/metrics.rs:167-170`：换手分母为初始资金，与 `core-02-research-workflow.md:26` 要求的“每日平均净资产”不一致。
- `crates/research-domain/src/plan.rs:70-95`：`plan_target` 按 `总资产 × 权重` 计算目标股数，但未校验所有行权重之和是否不超过 100%，也未扣除买入费用。
- `crates/research-domain/src/coordinator.rs:1386-1423`：`run_plan_compute` 中 `slots = min(top_k, 成员数)`、`weight = 1/slots`，但遍历全部成员；当成员数 > slots 时，总权重将超过 100%。
- `core-02-research-workflow.md:31-32`、`core-08-research-requirements.md:18`：已要求计划复用策略信号、目标总权重不超 100%、费用预留，但当前实现未对齐。

### 拟影响场景
- S14 比较、S15 交易计划。

### 验收缺口
- UT-S14-01/02 需补充几何年化、含初始本金的回撤、正确换手分母的断言；
- ST-S15-01 未验证计划与回测使用同一信号内核；
- 计划生成存在总权重可能超过 100% 的缺陷；
- 计划生成未做费用预留。

### 待外环裁决事项
1. 指标公式替换与计划权重修复，哪一项优先实施？
2. 交易计划是否必须复用已保存运行的信号结果，还是允许独立按模板参数重算后再与运行结果比对？
3. 费用预留的默认值与数据源（佣金/最低/税费）如何确定？

---

## 切片 4：数据闭环（Python 适配器 → Rust 快照 → 能力门控）

### 目标
- 将 Python 通达信/TickFlow 输出统一为 Rust 端可消费的标准暂存契约（CSV/Parquet + auxiliary JSON）；
- 原始价格与信号调整口径分离；
- 生成数据质量报告、记录来源与哈希、保证快照不可变；
- 财务数据按策略需要后续接入，并满足 `available_at` 时点与修订版本要求。

### 依赖
- `python/nautilus_trader/adapters/astock/*`
- `crates/research-domain/src/coordinator.rs` 的 `run_import`
- `crates/research-domain/src/auxiliary.rs`
- `core-08-research-requirements.md` 数据能力与范围

### 现有证据（静态）
- `python/nautilus_trader/adapters/astock/st_filter.py:65-74`：构造器仅保存 `_fin` 引用，`screen_pool` 使用调用者外部传入的 `fin_metrics`，**不存在自动抓取**，也未接入 Rust 快照 auxiliary 通道。
- `python/nautilus_trader/adapters/astock/financials_client.py:26-48`：`FinancialMetrics` 仅含业务字段，无 `available_at`、`revision`、`source_hash` 等时点/溯源字段。
- `crates/research-domain/src/coordinator.rs:2005-2120`：`run_import` 已支持行情、主档、财务、公司行为、日历、规则包六类 auxiliary 的 CSV 校验与规范化 JSON 内容对象写入；**S12 的 `universe.rs:375-406` 已能读取快照中的 `financial` auxiliary 并按 `available_at ≤ as_of` 取最新修订号**。
- `core-08-research-requirements.md:21-25`：明确要求每条记录保存业务可见时间与采集时间，修订值不得覆盖原历史版本；严格模式要求历史状态、交易日历、公司行为、费用/规则版本齐备。

### 能力/缺口区分
- **已有能力**：Rust 快照导入与 S12 规则引擎已支持财务 auxiliary、时点纪律、修订号取最大。
- **当前缺口**：Python 侧 ST 筛选器与财务客户端未使用上述统一契约；缺少从 Python 输出到 Rust auxiliary 的标准暂存桥接；缺少针对 Python 导入路径的质量报告（缺失率、来源、复权口径、哈希）。

### 拟影响场景
- S11 数据维护、S12 股票池。

### 待外环裁决事项
1. 是否将 ST 筛选器改为读取 Rust 快照中的 `financial` auxiliary，统一数据溯源？
2. 是否先为 Python 适配器增加“来源 + 哈希 + 质量报告”字段，再导入 Rust auxiliary？
3. 财务数据接入 S13 运行是否作为独立批次，还是先完成 Python → Rust 桥接？

---

## 切片 5：真实数据周频基线与持仓辅助

### 目标
- 以“先探索、再严格验证、再冻结规则向前模拟”的顺序建立真实数据周频基线；
- 提供手工持仓核对辅助，帮助用户将模拟计划与真实账户对齐；
- 明确区分合成演示结果与真实市场证据，不在任何输出中声称收益保证。

### 依赖
- 切片 1~4 的验收完成
- 真实 A股日线数据（通达信/TickFlow）
- 双平台 GUI 或 CLI 运行环境

### 现有证据（静态）
- `logos/resources/implementation/02-research-desktop-manifest.md:378-382`：列出环境阻塞项——双平台部署/smoke、真实数据性能量测待环境；但 `egui` 统一工作台骨架已在批次 10 落盘。
- `logos/resources/prd/2-product-design/1-feature-specs/core-02-research-workflow.md:34-44`：明确区分统一离线原型的演示范围与生产路径，声明“真实数据、样本外与稳健性均标记未运行”。
- `core-08-research-requirements.md:24-25`：严格模式要求历史状态、公司行为、费用/规则版本齐备；探索模式始终显示限制声明。
- `crates/research-desktop/src/bridge.rs:62-79`、`269-270`：运行表单默认 `mode = Strict`，但当前实现距离严格模式完整数据要求仍有差距。

### 能力/缺口区分
- **已有上报测试**：引擎单元基线、A股数据接入、S11~S15 部分 UT/ST 已在合成样本或 mock 下通过 reporter；不等于真实市场验证。
- **当前缺口**：无真实历史数据基线；无向前 walk-forward 模拟；无手工持仓导入与核对辅助的 UT/ST（除 `HoldingInput` 校验外）；双平台 GUI smoke 用例 `UT-S15-05/06`、`ST-S15-02/03` 当前为 skip/待环境。

### 拟影响场景
- S13~S15 全旅程、双平台 smoke 测试。

### 待外环裁决事项
1. 是否先使用真实历史数据跑一组最小周频基线，再冻结当前模板参数？
2. 向前模拟的步长、样本外比例、失败判定标准如何定义？
3. 持仓辅助功能（手工录入持仓、可卖数量、与计划差额核对）是否优先于自动规则扩展？

---

## 综述

以上五组切片按“研究正确性 → 成交与账户 → 指标与计划 → 数据闭环 → 真实基线”递进。每组均需独立变更提案（`openlogos change`），并按 OpenLogos 完整流程产出 delta、代码、UT/ST、reporter。当前 `astock-st-risk-filter` 提案仅覆盖切片 4 中“ST 风险筛选器 Python 现状收编与 reporter 接入”的最小一环，不改变切片 1~3 与 5 的实现状态。
