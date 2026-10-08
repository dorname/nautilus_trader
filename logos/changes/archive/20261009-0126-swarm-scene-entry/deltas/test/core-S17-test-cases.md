## ADDED — S17 场景入口用例（swarm-scene-entry 提案，issue #26）

# S17 场景入口用例（swarm-scene-entry 提案，crates/research-desktop tests，reporter 入账）

需求确认触发多 Agent 分析的场景入口口径：`do_confirm_requirement` 成功后必须
经 `SwarmWorker`（后台线程持 `SwarmBridge`，GUI 帧循环 channel 收发不阻塞渲染）
真实派发 S17 三契约；三路终态产物摘要必须挂到对应需求版本 R_n
（`RequirementVersion.swarm_analysis`），不得只停留在会话消息。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S17-21 | S17-AC-01 | 真实 LLM 经 SwarmWorker 派发三个最小 prompt 契约，收进度事件流直至终态 | 收到至少一次 Active 进度事件（不得只有终态）；终态事件为 Completed；三个子任务 Succeeded 且 output 非空；聚合摘要 attach_requirement_analysis 成功并可在 R_n 读回 |
| UT-S17-19 | S17-AC-01 | attach_requirement_analysis 判定矩阵：绑定正确 R 版本 / req_id 不存在 / 重复 attach | 摘要写入目标 R_n 且其他版本不受影响；req_id 不存在返回错误且不产生副作用；重复 attach 以最新摘要为准 |

### reporter
真实 LLM 调用产生 API 费用，prompt 最小化；SwarmWorker 事件经共享 reporter
写入 test-results.jsonl，fail 含 error，skip 不可计为通过。详见
core-09-research-test-cases.md。
