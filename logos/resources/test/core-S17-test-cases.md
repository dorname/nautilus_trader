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

# OUP 会话配置与项目隔离用例（oup-session-config 提案，crates/research-desktop tests，reporter 入账）

会话配置口径：session_id 必须绑定桌面项目 ID（`research-{project_id}`），
切换/新建项目后旧 worker 必须销毁重建；profile 从 base 数据目录解析
（禁止机器绝对路径硬编码），缺失时早失败；子进程以 workspace_root 为
cwd、stderr 落盘可排障、profile env_vars 显式注入。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-23 | S17-AC-01 | 无 LLM：两项目分别构造 AgentSessionConfig::desktop；GUI 安装 worker 后切换项目 | 两项目 session_id 与数据目录互不相同；切换/新建项目后 worker 被重置（下次任务绑定新 session）；在途任务引用被放弃并记录 |
| UT-S17-24 | S17-AC-01 | 无 LLM：AgentBridge::connect（spawn octos，不发起 LLM 调用） | 子进程 cwd 为 workspace_root；`{data_dir}/logs/octos-stderr.log` 落盘存在；profile env_vars 注入子进程环境（进程可见，不泄漏到日志） |

### reporter
纯本地配置/进程断言（octos serve 仅启动不调用 LLM，无 API 费用）；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。

# swarm 账本与聚合校验收尾用例（swarm-ledger-adr 提案，crates/research-desktop tests，reporter 入账）

崩溃恢复诚实口径：GUI/进程重启后，账本中未完成派发一律标记
「中断（进程重启）」——octos 子进程已随旧进程死亡，不得假装仍在运行；
聚合校验（M4.3 validator 语义）：三路产物全 Succeeded 且 output 非空才
允许挂载，否则阻断。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-25 | S17-AC-01 | 无 LLM：SwarmLedger 写入派发 → 终态更新；模拟重启（重开账本）读未完成项并标记中断 | 派发记录含 dispatch_id/契约/turn_id/状态；终态更新后可查；重启后未完成派发全部转为「中断（进程重启）」且不残留 Active |
| UT-S17-26 | S17-AC-01 | aggregate_valid 判定矩阵：全 Succeeded 有 output / 含 Failed / 含 Cancelled / output 空串 / 空结果集 | 仅全 Succeeded 且 output 非空返回 true；其余一律 false（阻断挂载） |

### reporter
纯本地账本/纯函数断言，无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。

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

## RD-003 动态效果与导航编排

| ID | 关联 | 输入 | 断言 |
|---|---|---|---|
| UT-GUI-05 | S17-AC-01 | 窄窗口遍历侧栏导航 | 图标导航保留可访问名称，点击仍进入对应页面，品牌名/分组/计数隐藏 |
| UT-GUI-06 | S17-AC-01 | 减少动态效果开启/关闭，任务指示器在不同时间绘制 | 开启时指示器静止，关闭时保持旋转；运行中、取消与终态文本及后台轮询不受影响；系统偏好读取失败采用静止默认值，并提供可访问的显式选择 |

新测试包含 OpenLogos reporter。系统偏好检测必须在启动阶段完成，不在 GUI 每帧执行外部命令；测试使用注入的偏好而不依赖宿主系统设置。


## 真实 LLM 验收配置选择

真实 LLM 测试与产品统一采用 RESEARCH_OCTOS_PROFILE 显式配置优先、用户 octos 默认配置兜底的机制。配置选择仅改变提供方与模型，不改变真实 turn、三契约派发、取消、终态及非空产物断言。临时配置不得写入仓库，不替换用户默认配置；报告记录实际模型及失败原因。UT-GUI-06 补充配置优先/默认兜底路径断言，ST-GUI-01/02 和既有真实 LLM 用例执行完整复验。

## 非交互派发固定输入

真实三路分析验收使用完整固定输入：日线收盘信号、下一交易日执行、已有 OHLCV、固定合成股票池、手续费与现金约束均已给定；子任务直接分析并返回结果，不向用户追问，不调用交互工具。该输入用于排除缺失前提导致模型请求人工回答的测试阻塞；仍要求三个真实 turn 成功终态、有效 UUID、非空持久化产物及完整 GUI 挂载，不得模拟终态或缩减三路断言。

## Server 保障与 LLM 配置用例（embed-octos-runtime，crates/research-desktop tests/，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-28 | S17-AC-01 | 无 LLM：`backend_ensure` 三级解析（内嵌资源 → `~/.octos/bin` → PATH）与 `REQUIRED_OCTOS_RELEASE` 版本 pin | 三级顺序正确且记录实际来源；非内嵌来源版本不匹配时返回中文诊断错误（含安装指引），不静默继续；三级全缺失时给出引导错误，spawn 被拒绝 |
| UT-S17-29 | S17-AC-01 | 无 LLM：`AgentSessionConfig::desktop()` profile 复制失败（源缺失/目标不可写）不再 `let _` 吞错 | 复制失败显式报错并给出引导文案（提示 RESEARCH_OCTOS_PROFILE 与 ~/.octos 两个来源）；profile 档位发现能列出 `data_dir/profiles/*.json` 的全部档位 ID；单档/空目录/不可读目录均不 panic |

### reporter

纯本地解析/校验断言，无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。

## 产物文件型契约与工作区回流用例（embed-octos-runtime，crates/research-desktop tests/，真实 LLM，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S17-23 | S17 多 Agent 分析闭环 | 真实 LLM：确认需求派发产物文件型三路契约（每路在托管工作区写分析文件）；终态后桌面从 `<instance>/profiles/<profile>/data/users/<user>/workspace/` 回流 | 三路 turn 真实终态；托管工作区出现契约声明的产物文件且非空白；产物文本挂载到需求版本 R_n；文件缺失/空白的一路按失败处理且不挂载冒充文本；`aggregate_valid` 聚合口径不变 |

### reporter

真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择（与
wire_real_llm 同口径）；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。

## 本地启动脚本与日志落盘用例（local-launch-logs-20261012，crates/research-desktop + scripts/，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S17-30 | S17-AC-01 | 无 LLM：`logging::init_file_logger` 指定 tmp 日志目录初始化后，`logline` 写消息 | 目录自动创建；生成 `research-desktop-<时间戳>.log` 会话文件；logline 后文件内容含该消息与 `[HH:MM:SS]` 时间戳；stderr 行为不变 |
| UT-S17-31 | S17-AC-01 | 无 LLM：`scripts/run-desktop.sh` 静态检查 | 脚本存在且可执行；`bash -n` 语法校验通过；内容含 WSLg 修正（XDG_RUNTIME_DIR / 去 WAYLAND_DISPLAY）、`logs` 目录创建与 tee 落盘逻辑、libxkbcommon-x11 预检 |

### reporter

纯本地断言（tmp 目录隔离，无外部调用）；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。

## 在线化口径注记（online-chat-llm-config-20261012）

S17 的「离线预设意图」口径自 S22 起修订：意图路由的确定性业务动作（含本文件既有用例覆盖的确认需求三路、生成设计/代码、整理参数）不变；**自由问答兜底由 ai.rs 预设文案改为真实 LLM 经 octos 回复**（octos 不可用时才回退预设并如实标注）。本文件既有用例断言不涉及兜底文案，无需修改；在线对话与模型配置用例见 core-S22-test-cases.md。
