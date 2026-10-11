# S18 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S18-11 | 设计图与说明 | 同源六节点、信号/执行时点可见；节点可定位源码 | [manual] |
| ST-S18-12 | 代码修复和版本 | 先显示差异再应用；自定义源码不能运行；保存不可变v1/v2 | [manual] |
| ST-S18-13 | 上游过期 | 需求、设计或资源更新阻止旧版本新运行；历史实验不变 | [manual] |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果——生产验收口径中本节用例标记 [manual] 排除，原型证据以 prototype-review JSONL 为准。

## GUI 落地口径（astock-desktop-launch 提案，批次 L4）

生产桌面应用（crates/research-desktop）实现本节旅程：可自动化部分（差异先显后应用、
自定义源码不可运行、不可变版本保存与上游过期判定的状态机与纯函数）由
crates/research-desktop 的 UT/ST 承载并接 OpenLogos reporter；图形渲染与交互旅程
保留人工执行（[manual] 标注不变）。原型证据保留于归档 prototype-review/。

### 生产用例执行（批次 L4 落地，crates/research-desktop tests/s17_s20_ai.rs，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S18-15 | S18-AC-01 | 版本冻结与上游过期：无需求生成设计；保存版本冻结 R/D 引用与修订戳；同源码重复保存；确认新需求后旧版本新运行 | 无需求生成设计拒绝；版本不可变（幂等保存不产生新冻结戳）；过期版本拒绝新实验且错误指向上游过期；历史实验记录不变；恢复冻结引用仍过期（冻结戳落后即过期） |

## 业务逻辑补充检查

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S18-14 | 修改需求、设计和源码草稿后切页 | 提示未确认范围，冻结输入不变；确认上游后版本过期 | [manual] |

补充检查写入 prototype-review/business-test-results.jsonl，采用相同 OpenLogos reporter 字段，仅作为原型证据，生产验收口径中标记 [manual] 排除；GUI 落地后其业务规则部分（未确认范围提示与版本过期判定）由 research-desktop UT 承载。

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

## 协议类型化与工具通知用例（embed-octos-runtime，crates/research-desktop tests/，reporter 入账）

| ID | 关联 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S18-17 | S18-AC-01 | 无 LLM：OUP 客户端 spawn 前 backend_ensure 版本 pin 校验；解析 `tool/started`/`tool/completed` 通知 | 版本 pin 匹配才允许 spawn（不匹配给中文诊断）；tool 通知解析为结构化事件（tool_call_id、名称、终态）供任务条消费；未知通知类型不 panic、不中断读循环；既有通知路径（turn/completed、assistant_persisted、projection/envelope）行为不变 |
| UT-S18-18 | S18-AC-01 | 无 LLM：闭环权限默认档与 Full-Access 决策门 | 会话默认权限档为 Workspace-Write（spawn 参数不携带任何免沙箱标志）；Full-Access 仅在 GUI 显式二次确认后写入 spawn 参数，任何默认路径/测试夹具均不携带 `--danger-full-access`；权限档选择状态持久化且项目切换不迁移 |

### reporter

纯本地断言（mock 通知流），无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。

## 工具可见性与产物回流用例（embed-octos-runtime，crates/research-desktop tests/，真实 LLM，reporter 入账）

| ID | 关联 | 输入 | 断言 |
|---|---|---|---|
| ST-S18-17 | S18-AC-01 | 真实 LLM：生成设计任务（产物文件型），观察工具通知与产物回流 | 任务条出现工具执行行（数据来自 tool/started|completed 通知）；终态后托管工作区存在设计草稿文件且非空白；产物文件内容进入设计说明编辑区；文件缺失按失败处理，不用 envelope 文本冒充 |

### reporter

真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择（与
wire_real_llm 同口径）；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。
