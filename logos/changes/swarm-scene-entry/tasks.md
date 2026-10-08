# 实现任务

## [delta] 规格变更
- [x] 产出 `deltas/test/core-S17-test-cases.md` — 新增 S17 场景入口用例：ST-S17-21（真实 LLM 经 SwarmWorker 派发三契约最小 prompt：进度事件流 → 终态 Completed → 三 output 非空 → 摘要 attach 到需求版本）、UT-S17-19（attach_requirement_analysis 判定矩阵：绑定正确 R 版本 / req_id 不存在拒绝 / 重复 attach 以最新为准）
- [x] 产出 `deltas/test/core-S19-test-cases.md` — 新增 S19 参数整理用例：ST-S19-15（真实 LLM 整理参数 → parse_run_params → 真实协调器合成数据跑通 → 实验绑定 task_id 且数值来自协调器）、UT-S19-16（parse_run_params 判定矩阵：合法 / fence 包裹 / 非 JSON / 缺字段 / fast>=slow / top_k=0 / 非正金额 / 非法日期）

## [code] 代码实现

- [ ] 实现代码变更