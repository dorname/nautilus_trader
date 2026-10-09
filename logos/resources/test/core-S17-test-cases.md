# S17 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S17-11 | 统一入口与资源导航 | 旧入口锚点映射到同一应用，导航不丢失会话 | [manual] |
| ST-S17-12 | 需求确认与项目隔离 | 空正文/越界拒绝；新项目独立；切回保留版本和消息 | [manual] |
| ST-S17-13 | 对话、取消与失败重试 | 未知意图解释限制；取消不提交；模拟失败后可重试 | [manual] |
| ST-S17-14 | 布局与输入安全 | 1440/1100无水平溢出；用户HTML按文本显示；专注切换可用 | [manual] |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果——生产验收口径中本节用例标记 [manual] 排除，原型证据以 prototype-review JSONL 为准。

## GUI 落地口径（astock-desktop-launch 提案，批次 L4）

生产桌面应用（crates/research-desktop）实现本节旅程：可自动化部分（意图路由、项目
隔离、取消无产物、失败重试的状态机与纯函数）由 crates/research-desktop 的 UT/ST
承载并接 OpenLogos reporter；渲染、布局与输入焦点的旅程验收保留人工执行（[manual]
标注不变，不因 GUI 落地自动转为已通过）。原型 HTML 编排检查结果（13+3 pass）作为
设计期证据保留于归档 prototype-review/。

### 生产用例执行（批次 L4 落地，crates/research-desktop tests/s17_s20_ai.rs，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-16 | S17-AC-01 | 离线意图路由（11 预设意图 + 未知输入）；新建/切换项目往返；需求确认四类非法输入 | 未知意图诚实拒绝且不编造；意图→子视图路由正确；项目会话/需求/版本隔离、切回保留；空正文/空验收标准/投入比例越界/负成交额拒绝，合法输入生成 R 版本 |
| ST-S17-16 | S17-AC-01 | AI 工作台全旅程（长驻协调器）：对话路由→需求→设计→版本冻结→真实实验→报告→计划核对→确认导出→上游过期→新版本取消 | 实验经真实协调器终态成功；报告演示检查可过且正式验证恒「证据不足」；导出 CSV 落盘含演示标识且与确认内容一致；过期版本拒绝新实验且历史实验不变；取消直达（Cancelling 或 ALREADY_TERMINAL 容双态），终态一致 |

## 真实 LLM 接线用例（wire-real-llm-oup 提案，crates/research-desktop tests/wire_real_llm.rs，reporter 入账）

真实 LLM（moonshot-coding profile）经 OUP 全链路：session/open → turn/start →
事件流（message/delta、projection/envelope）→ turn/completed 终态。回复文本只从
`projection/envelope` 中 `type=assistant_persisted` 的持久化产物提取（RD-018：
禁止拼接 message/delta）；demo_task 演示路径整体移除（RD-008/RD-030）。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S17-17 | S17-AC-01 | 真实 LLM turn 全生命周期：connect → submit_and_wait（最小 prompt，120s 超时） | 收到 turn/completed（非 turn/error）；turn_id 为合法 UUID；assistant_persisted 提取到非空回复文本 |
| ST-S17-18 | S17-AC-01 | 同会话连续两次真实 turn | 两个 turn_id 均为合法 UUID 且互不相同；两次均收到 turn/completed |
| UT-S17-17 | S17-AC-01 | 源码静态检查：crates/research-desktop/src 不得存在 DemoTask / DemoAction / demo_task 路径 | 全量扫描无命中；AgentBridge 不依赖 demo_task（编译期断言） |

### reporter
真实 LLM 调用产生 API 费用，prompt 最小化（如「回复 ok」）；超时 120 秒；结果经共享
reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。详见
core-09-research-test-cases.md。

## 业务逻辑补充检查

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S17-15 | 完整旅程、失败报告、上游过期、当前版本未运行 | 下一步按有效引用与证据计算，不按历史数量；计划核对后提示导出 | [manual] |

补充检查写入 prototype-review/business-test-results.jsonl，采用相同 OpenLogos reporter 字段，仅作为原型证据，生产验收口径中标记 [manual] 排除；GUI 落地后其业务规则部分（有效引用与证据计算）由 research-desktop UT 承载。

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

# S17 GUI 无头走查用例（gui-walkthrough 提案，crates/research-desktop tests/gui_walkthrough.rs，reporter 入账）

issue #1/#32 环境下 WSLg 无可见窗口，S17 场景入口的 GUI 旅程经
egui_kittest（AccessKit 无头驱动真实 render_root）自动化走查；
关键帧经 wgpu 软件渲染截图落盘 target/gui-walkthrough/ 作为证据。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-GUI-01 | S17-AC-01 | 无头点击「确认需求」（真实 LLM）→ 观察任务条 → 泵帧至终态 | 任务条出现「多 Agent 分析 R1 · n/3 · Ns」三路进度行；终态产出完成卡（不得是「未全部成功」）；R1 需求版本挂载非空 swarm_analysis 摘要；截图含任务条与完成卡两个关键帧 |

### reporter
真实 LLM 调用产生 API 费用，prompt 为 S17 标准三契约；结果经共享 reporter
写入 test-results.jsonl，fail 含 error，skip 不可计为通过。
