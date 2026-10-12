## ADDED — 宽限收割与空产物分类用例（fix-reallm-stability-20261012，crates/research-desktop，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S18-19 | S18-AC-01 | 无 LLM：OUP `turn_wait_completed` 宽限收割——`turn/completed` 先到达且 `assistant_text` 为空时，不立即返回，进入宽限窗口（≤3s）继续收割；迟到的 `assistant_persisted` 在窗口内到达 | 返回的 `TurnOutcome.assistant_text` 为迟到 persisted 的文本；宽限窗口内收集的 tool 事件不丢失；终态语义不变（completed payload 原样返回） |
| UT-S18-20 | S18-AC-01 | 无 LLM：宽限期满仍无 `assistant_persisted` | 宽限期满（≤3s）后返回 completed、`assistant_text=None`（不无限等待、不超总超时语义）；channel 断开时提前结束宽限并按既有 ProcessExited 语义返回 |

### reporter

纯本地断言（channel 注入 mock 通知流，无外部调用）；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。
