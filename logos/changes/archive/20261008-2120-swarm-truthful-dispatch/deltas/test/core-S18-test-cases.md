## ADDED — Pipeline 真实串行注入用例（swarm-truthful-dispatch 提案）

# Pipeline 真实串行注入用例（swarm-truthful-dispatch 提案，crates/research-desktop tests/m3_pipeline_integration.rs，reporter 入账）

S18 设计→代码检查 Pipeline 的正确语义：stage 串行执行，上游 stage 真实
turn/completed 后其 assistant_text 注入下游 prompt 的 `{pipeline_input}` 占位符
（RD-011：previous_output 恒空为缺陷）；任一 stage 失败立即终止，未派发 stage
不得标成功。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S18-16 | S18-AC-01 | 真实 LLM 两 stage Pipeline（s18_design_to_code_contracts，最小化 prompt），dispatch_pipeline 串行执行至终态 | stage2 实际发送的 final_prompt 包含 stage1 的 output 文本且不含 `{pipeline_input}` 字面量；stage2 派发晚于 stage1 完成（串行）；两 stage 均 Succeeded 且 output 非空；任一失败时总体 Failed 且下游未派发为 Cancelled |

### reporter
真实 LLM 调用产生 API 费用，prompt 最小化；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。详见
core-09-research-test-cases.md。
