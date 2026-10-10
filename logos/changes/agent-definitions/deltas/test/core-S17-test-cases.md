## ADDED — Agent 定义清单用例（agent-definitions 提案，issue #13/#22）

# Agent 定义清单用例（agent-definitions 提案，crates/research-desktop tests，reporter 入账）

能力声明口径：`logos/agents/*.toml` 为权威 AgentDefinition 清单（编译期
内嵌，运行时不依赖仓库文件）；派发契约声明 definition_id 时必须在清单中
存在，未知 id 拒绝派发（工具白名单约束力落在客户端校验）；experiment-runner
的 disallowed_tools 必须显式禁止协调器调用（issue #22 ADR）。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-27 | S17-AC-01 | 无 LLM：内嵌 manifest 解析；S17 三契约 definition_id 校验；未知 definition_id 派发 | 四定义（requirement-analyst/design-generator/code-reviewer/experiment-runner）齐全且字段解析成功；S17 三契约绑定的 id 存在；experiment-runner disallowed_tools 含协调器调用禁止项；未知 definition_id 的 dispatch 被拒绝且无副作用 |

### reporter
纯本地解析/校验断言，无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。
