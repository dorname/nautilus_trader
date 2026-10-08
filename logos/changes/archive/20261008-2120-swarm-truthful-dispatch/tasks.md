# 实现任务

## [delta] 规格变更
- [x] 产出 `deltas/test/core-S17-test-cases.md` — 新增 Swarm 真实终态用例：ST-S17-19（并发派发等 turn/completed，三契约全 Succeeded 且 output 非空）、ST-S17-20（派发后首轮轮询保持 Active，不得假 Completed/Succeeded）、UT-S17-18（终态归约纯函数：空集/全成功/含失败/含取消的判定矩阵，失败不得吞为成功）
- [x] 产出 `deltas/test/core-S18-test-cases.md` — 新增 Pipeline 注入用例：ST-S18-16（串行执行：stage2 开始晚于 stage1 完成；stage2 final_prompt 含 stage1 输出且不含 `{pipeline_input}` 字面量；两 stage 均 Succeeded 且 output 非空）

## [code] 代码实现

- [x] oup.rs：新增 `TurnTerminal` 枚举与 `OupClient::poll_turn_once`（非阻塞泵事件流，复用 assistant_persisted/turn 终态匹配器）
- [x] agent_bridge.rs：新增 `active_turn_id`/`pending_assistant_text` 追踪字段与 `AgentBridge::poll_turn_outcome`（终态后回落 Connected）
- [x] swarm_bridge.rs：`SubtaskResult` 增加 `turn_id`/`final_prompt`；新增纯函数 `reduce_dispatch_terminal`（UT-S17-18 判定矩阵）；重写 `poll_all`（终态以 turn 事件流为准，task/list 仅观测，issue #12）；重写 `dispatch_pipeline`（串行 submit_and_wait + 上游产物真实注入 `{pipeline_input}`，失败即终止、下游 Cancelled，issue #11/RD-011）
- [x] 测试改造：m2 新增 ST-S17-19/ST-S17-20/UT-S17-18（case() reporter 入账）；m3 新增 ST-S18-16 并将 st_m3_01 断言改为终态口径（prompt 最小化）、st_m3_02 降为契约结构校验（重量级模板 e2e 由 ST-S18-16 承载）；m4 st_m4_01/02 等真实终态且 output 非空
- [x] 超时工程化：Pipeline 单 stage 超时 600s（`PIPELINE_STAGE_TIMEOUT_SECS`，吸收 moonshot 尾延迟 150s+）；m4 等待放宽 300s
- [x] 本地回归：m2 8/8、m3 5/5、m4 4/4 全绿（真实 LLM，串行）
