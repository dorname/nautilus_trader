## ADDED — S19 参数整理用例（swarm-scene-entry 提案，issue #15）

# S19 参数整理用例（swarm-scene-entry 提案，crates/research-desktop tests/m4_experiment_integration.rs，reporter 入账）

LLM 在 S19 的职责边界：只做意图/参数整理（输出结构化 JSON 表单参数），
实验数值必须来自真实协调器任务（TaskView/equity）并绑定 task_id；
`parse_run_params` 严格校验，LLM 输出不得含任何结果数值，非法输入一律拒绝。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S19-15 | S19-AC-01 | 真实 LLM 按 s19_organize_params_prompt 整理参数 → parse_run_params → build_run_form → 真实协调器（合成数据快照 + 股票池）submit_run 至任务终态 | LLM 回复解析成功且只含表单参数；协调器任务终态成功；实验记录绑定真实 task_id；净值/收益数值来自协调器 TaskView/equity 而非 LLM 文本 |
| UT-S19-16 | S19-AC-01 | parse_run_params 判定矩阵：合法 JSON / ```json fence 包裹 / 非 JSON / 缺字段 / fast>=slow / top_k=0 / 非正金额 / 非法日期 | 合法与 fence 包裹解析成功；其余一律拒绝并给出中文字段级错误；解析输出不含任何结果数值字段 |

### reporter
真实 LLM 调用产生 API 费用，prompt 最小化；协调器使用合成数据临时工作区；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。
详见 core-09-research-test-cases.md。
