## MODIFIED — 单元用例（无 LLM，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S22-01 | S22-AC-02 | OUP AppUI 方法族封装：`llm_catalog/list/upsert/select/test` 请求序列化与响应解析 | upsert params 仅含契约字段（profile_id/selection/api_key/set_primary；selection 内 family_id/model_id/route{route_id,label,base_url,api_key_env,api_type}），未知字段不发送；catalog/list/test 响应类型化解析；错误响应（含 #2166 typed unknown-field 诊断）原样透传 |
| UT-S22-02 | S22-AC-01 | L0/L1 入口与自由问答改道（纯函数/状态机，无 worker） | L0 Unknown 且 octos 可用时先产生 TriageTranslate 而非直接 LlmChat；只有结构化 `domain=out_of_scope` 归约为 LlmChat；域内 Artifact/ClarificationRequested 不进入 ChatFree；确定性动作意图仍本地分发；octos 不可用时 Unknown 降级 OfflinePreset |
| UT-S22-03 | S22-AC-02 | 模型设置浮层表单校验与脱敏 | base_url 空 / api_type 非枚举 / family 未选时保存被阻断并给中文提示；api_key 输入不回显、不出现在运行记录与日志字符串；保存后浮层状态复位 |

## MODIFIED — 场景用例（真实 LLM / 真实 octos，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| ST-S22-01 | S22-AC-01 | 真实 LLM 自由对话：通用问题先经生产 Triage prompt 判定域外，再串行派发 ChatFree 至 turn 完成 | Triage 持久化产物为合法 out_of_scope 结构；随后任务条出现「对话回复 ·」行并收尾；消息流新增助手回复且非空、**不含**离线预设兜底文案；ChatFree 的 turn/completed 与 assistant_persisted 终态口径不变 |
| ST-S22-02 | S22-AC-02 | provider 配置闭环（临时实例数据目录隔离，不动用户真实 profile）：catalog → upsert（复用现有 family 的 route，api_key 经环境变量）→ test → list | catalog 非空且含已知 family；upsert 返回 applied；test 连通成功；list 含新配置且 set_primary 生效；测试后实例目录可整体删除 |

### S24 兼容性注记

UT-S22-04/05 与 ST-S22-03 的流式预览、持久化替换和任务进度断言不变，作用对象是 ChatFree 及既有文本生成任务；TriageTranslate 的结构化 delta 不进入聊天预览。新增 Triage 校验、澄清与风险门用例由 S24 承担，不在本节重复编号。
