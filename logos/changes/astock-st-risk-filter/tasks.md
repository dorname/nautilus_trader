# 实现任务

## [delta] 规格变更
- [x] 产出 delta 文件到 `deltas/prd/1-product-requirements/07-astock-requirements.md` — 新增 FR-A9 ST 风险过滤需求，并明确当前实现边界（无 `unknown` 枚举、无 `available_at`/`revision`、不保证会计年度连续亏损、禁止用于严格历史研究）
- [x] 产出 delta 文件到 `deltas/prd/2-product-design/1-feature-specs/core-02-research-workflow.md` — 在“筛选语义”章节明确 ST 风险筛选器为独立 Python 辅助模块、内部 API 契约、调用时序、数据不足口径，与 S12 三态引擎不接线
- [x] 产出 delta 文件到 `deltas/test/astock/astock-test-cases.md` — 新增 UT-STF-01~UT-STF-11 与 ST-STF-01 用例，覆盖财务判据、行情判据、数据保守排除、年报/季报混合、缺失字段边界、reporter 阶段聚合
- [x] 产出 delta 文件到 `deltas/scenario/core-st-risk-orchestration.json` — 新增 ST-STF-01 内部 API 编排规格（`screen_pool` → `apply_pool` 调用序列），沿用 A股 S10 辅助子流程，不新增业务场景编号

## [code] 代码实现
- [ ] 更新 `python/tests/adapters/astock/test_st_filter.py` — 将测试函数命名对齐 OpenLogos 用例 ID（`test_ut_stf_*` / `test_st_stf_*`），并新增 UT-STF-09（年报+季报混合亏损，验证当前按最近 N 期 hard 及限制）、UT-STF-10（缺失字段/无 available_at/revision 现状边界）；**不修改 `st_filter.py` 业务行为**
- [ ] 更新 `python/tests/adapters/astock/conftest.py` — 修正跨 pytest 阶段状态聚合：setup/call/teardown 取最差状态，call/setup 失败、skip、teardown 失败均正确映射为 JSONL 状态；新增 UT-STF-11 真实子进程回归用例，覆盖成功/调用失败/setup 失败/skip/teardown 失败
- [ ] 运行真实 pytest 子进程验证新增/重命名用例通过，并检查 `logos/resources/verify/test-results.jsonl` 中各状态与 error 字段正确落账（实跑证据由 verify 落账）
