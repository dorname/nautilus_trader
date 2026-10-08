## ADDED — Swarm 真实终态用例（swarm-truthful-dispatch 提案）

# Swarm 真实终态用例（swarm-truthful-dispatch 提案，crates/research-desktop tests/m2_swarm_integration.rs，reporter 入账）

多 Agent 并发派发的终态口径：以 turn 事件流（turn/completed / turn/error）为准，
禁止以 task/list 空列表判定成功（RD-012）；未经真实终态且无产物的子任务不得标
Succeeded。派发后持续轮询直至全部子任务终态或超时（默认 180s）。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S17-19 | S17-AC-01 | 真实 LLM 三契约并发派发（S17 数据可行性/反例检索/约束核对），dispatch_parallel 后带超时循环 poll_all 至终态 | 三个 turn_id 均为合法 UUID；终态后三个子任务全部 Succeeded 且每个 output 非空；SwarmDispatchState 为 Completed |
| ST-S17-20 | S17-AC-01 | 派发后立即执行一次 poll_all（真实 LLM turn 进行中） | 首轮轮询后 SwarmDispatchState 仍为 Active，不得置 Completed；Running 子任务不得标 Succeeded；output 仍为 None |
| UT-S17-18 | S17-AC-01 | 终态归约纯函数判定矩阵：空集 / 全 Succeeded / 含 Failed / 含 Cancelled / 含 Running | 空集与含 Running 不得判终态；含 Failed 或 Cancelled 归约为 Failed（不得吞为 Completed）；仅全 Succeeded 归约为 Completed |

### reporter
真实 LLM 调用产生 API 费用，prompt 最小化；并发上限 MAX_PARALLEL_DISPATCH=3，
每子 agent 独立数据目录；结果经共享 reporter 写入 test-results.jsonl，fail 含
error，skip 不可计为通过。详见 core-09-research-test-cases.md。
