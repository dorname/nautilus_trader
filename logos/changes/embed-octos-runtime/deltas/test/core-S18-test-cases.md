# Delta: core-S18-test-cases.md

## ADDED — 协议类型化与工具通知用例（embed-octos-runtime，crates/research-desktop tests/，reporter 入账）

| ID | 关联 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S18-17 | S18-AC-01 | 无 LLM：OUP 客户端 spawn 前 backend_ensure 版本 pin 校验；解析 `tool/started`/`tool/completed` 通知 | 版本 pin 匹配才允许 spawn（不匹配给中文诊断）；tool 通知解析为结构化事件（tool_call_id、名称、终态）供任务条消费；未知通知类型不 panic、不中断读循环；既有通知路径（turn/completed、assistant_persisted、projection/envelope）行为不变 |
| UT-S18-18 | S18-AC-01 | 无 LLM：闭环权限默认档与 Full-Access 决策门 | 会话默认权限档为 Workspace-Write（spawn 参数不携带任何免沙箱标志）；Full-Access 仅在 GUI 显式二次确认后写入 spawn 参数，任何默认路径/测试夹具均不携带 `--danger-full-access`；权限档选择状态持久化且项目切换不迁移 |

### reporter

纯本地断言（mock 通知流），无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。

## ADDED — 工具可见性与产物回流用例（embed-octos-runtime，crates/research-desktop tests/，真实 LLM，reporter 入账）

| ID | 关联 | 输入 | 断言 |
|---|---|---|---|
| ST-S18-17 | S18-AC-01 | 真实 LLM：生成设计任务（产物文件型），观察工具通知与产物回流 | 任务条出现工具执行行（数据来自 tool/started|completed 通知）；终态后托管工作区存在设计草稿文件且非空白；产物文件内容进入设计说明编辑区；文件缺失按失败处理，不用 envelope 文本冒充 |

### reporter

真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择（与
wire_real_llm 同口径）；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。
