# S22 测试用例（online-chat-llm-config-20261012）

## 验收条件

- **S22-AC-01**：自由问答由真实 LLM 经 octos 回复；确定性业务动作不经 LLM；octos 不可用时诚实降级离线预设并如实标注。
- **S22-AC-02**：GUI 内可完成 provider 配置闭环（catalog → upsert → test → list 可见）；api_key 仅 RPC 内存传输，不写盘不进日志。

## 单元用例（无 LLM，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S22-01 | S22-AC-02 | OUP AppUI 方法族封装：`llm_catalog/list/upsert/select/test` 请求序列化与响应解析 | upsert params 仅含契约字段（profile_id/selection/api_key/set_primary；selection 内 family_id/model_id/route{route_id,label,base_url,api_key_env,api_type}），未知字段不发送；catalog/list/test 响应类型化解析；错误响应（含 #2166 typed unknown-field 诊断）原样透传 |
| UT-S22-02 | S22-AC-01 | 意图路由兜底改道（纯函数/状态机，无 worker） | 未知意图产生自由对话派发决策（非预设兜底文案）；确定性动作意图（实验/报告/计划等）仍路由本地动作；octos 不可用态下未知意图降级预设回复 |
| UT-S22-03 | S22-AC-02 | 模型设置浮层表单校验与脱敏 | base_url 空 / api_type 非枚举 / family 未选时保存被阻断并给中文提示；api_key 输入不回显、不出现在运行记录与日志字符串；保存后浮层状态复位 |

## 场景用例（真实 LLM / 真实 octos，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| ST-S22-01 | S22-AC-01 | 真实 LLM 自由对话：发送通用问题（非业务意图），等待 turn 完成 | 任务条出现「对话回复 ·」行并收尾；消息流新增助手回复且非空、**不含**离线预设兜底文案；turn/completed 终态 |
| ST-S22-02 | S22-AC-02 | provider 配置闭环（临时实例数据目录隔离，不动用户真实 profile）：catalog → upsert（复用现有 family 的 route，api_key 经环境变量）→ test → list | catalog 非空且含已知 family；upsert 返回 applied；test 连通成功；list 含新配置且 set_primary 生效；测试后实例目录可整体删除 |

### reporter

UT 纯本地断言（mock/纯函数）；ST 真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。

## ADDED — 流式回显与进度细化用例（fix-issues-40-41-20261013 / issue #41）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S22-04 | S22-AC-01 | 无 LLM：流式预览状态机（mock message/delta 帧经 AgentWorker 推送 GUI） | delta 到达即更新右栏预览并标记「生成中」；turn/completed 后预览被 assistant_persisted 文本替换，「生成中」标记消除；全程无 delta 时终态一次性回显（诚实降级）；异 turn_id delta 不混入当前预览 |
| UT-S22-05 | S22-AC-01 | 无 LLM：任务条进度细化纯函数 | 流式中对话任务条含「接收中」与已收增量字数；多 Agent 任务条含每路子任务状态（执行中/已完成/失败）；单路子任务失败即时不等全批完成也可见 |
| ST-S22-03 | S22-AC-01 | 真实 LLM 自由对话流式观察：发送通用问题（非业务意图）至 turn 完成，记录 delta 帧计数与预览行为 | 终态文本与 assistant_persisted 持久化产物一致且非空；若 delta 计数 >0，预览曾按增量增长且终态替换后无重复文本；若计数为 0（该 profile 不发 delta），如实记录降级为一次性回显——不计失败但结果中必须标注 delta 计数 |

### reporter

UT 纯本地断言（mock/纯函数）；ST 真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。
