# 实现任务

## [delta] 规格变更
- [x] 产出 `deltas/test/core-S17-test-cases.md` — 新增 S17 场景入口用例：ST-S17-21（真实 LLM 经 SwarmWorker 派发三契约最小 prompt：进度事件流 → 终态 Completed → 三 output 非空 → 摘要 attach 到需求版本）、UT-S17-19（attach_requirement_analysis 判定矩阵：绑定正确 R 版本 / req_id 不存在拒绝 / 重复 attach 以最新为准）
- [x] 产出 `deltas/test/core-S19-test-cases.md` — 新增 S19 参数整理用例：ST-S19-15（真实 LLM 整理参数 → parse_run_params → 真实协调器合成数据跑通 → 实验绑定 task_id 且数值来自协调器）、UT-S19-16（parse_run_params 判定矩阵：合法 / fence 包裹 / 非 JSON / 缺字段 / fast>=slow / top_k=0 / 非正金额 / 非法日期）

## [code] 代码实现

- [x] swarm_bridge.rs：新增 `SwarmWorker`（后台线程持 SwarmBridge，channel 收发进度事件，对齐 AgentWorker 模式；300s 总超时真实取消不吞成功）；新增 `parse_run_params` 纯函数（严格校验，输出只含表单参数，结果数值字段一律拒绝）、`build_run_form`、`s19_organize_params_prompt`、`summarize_swarm_results`
- [x] workspace.rs：`RequirementVersion` 增 `swarm_analysis: Option<String>`；新增 `attach_requirement_analysis(req_id, summary)`
- [x] app.rs：`do_confirm_requirement` 成功后自动派发 S17 三契约；帧循环 `poll_swarm_task` 非阻塞更新；终态聚合摘要 attach 到 R 版本并 tell_card；任务条新增三路进度行（n/3）与本地放弃取消；运行页新增「AI 整理参数」按钮（AgentWorker turn → parse → 填 RunForm → 二次确认走现有 submit_run，不自动提交）
- [x] 测试：m2 新增 ST-S17-21；s17_s20_ai 新增 UT-S17-19；m4 新增 ST-S19-15 / UT-S19-16；全部 case() reporter 入账 test-results.jsonl
- [x] 本地回归：m2 9/9、m4 6/6、s17_s20_ai 6/6 全绿（真实 LLM 用例串行执行）
