## ADDED — S17 取消与中断用例（turn-lifecycle 提案，issue #19/#28/#29）

# S17 取消与中断用例（turn-lifecycle 提案，crates/research-desktop tests，reporter 入账）

取消语义诚实口径：取消必须经 `turn/interrupt` 按 turn_id 真实中断；只有
中断请求真实发出的子任务才标 Cancelled；中断发不出的标 Failed 且整体
返回错误；空 tasks/本地状态机不得伪装成功取消（issue #28）。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-20 | S17-AC-01 | 无 LLM 判定矩阵：无活跃 turn 调 interrupt_turn；cancel_all 于 无 Running / interrupt 全发出 / 部分发出 三种情形 | 无活跃 turn 明确拒绝且不改状态；全发出 → 子任务全 Cancelled 且整体 Ok；部分发出 → 未发出标 Failed、整体返回 Err 且不得全标 Cancelled |
| ST-S17-22 | S17-AC-01 | 真实 LLM 长 prompt 派发（SwarmBridge dispatch_parallel）→ 立即 cancel_all（turn/interrupt）→ poll 至终态 | 中断后子任务终态为 Cancelled（非 Succeeded）；终态归约为 Failed（不得吞为 Completed）；不得出现「已取消但 turn 仍 active」的假成功 |

### reporter
ST 调用真实 LLM 产生 API 费用，interrupt 后模型侧以 interrupted 收尾；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。
