## ADDED — 流式回显与进度细化用例（fix-issues-40-41-20261013 / issue #41）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S22-04 | S22-AC-01 | 无 LLM：流式预览状态机（mock message/delta 帧经 AgentWorker 推送 GUI） | delta 到达即更新右栏预览并标记「生成中」；turn/completed 后预览被 assistant_persisted 文本替换，「生成中」标记消除；全程无 delta 时终态一次性回显（诚实降级）；异 turn_id delta 不混入当前预览 |
| UT-S22-05 | S22-AC-01 | 无 LLM：任务条进度细化纯函数 | 流式中对话任务条含「接收中」与已收增量字数；多 Agent 任务条含每路子任务状态（执行中/已完成/失败）；单路子任务失败即时不等全批完成也可见 |
| ST-S22-03 | S22-AC-01 | 真实 LLM 自由对话流式观察：发送通用问题（非业务意图）至 turn 完成，记录 delta 帧计数与预览行为 | 终态文本与 assistant_persisted 持久化产物一致且非空；若 delta 计数 >0，预览曾按增量增长且终态替换后无重复文本；若计数为 0（该 profile 不发 delta），如实记录降级为一次性回显——不计失败但结果中必须标注 delta 计数 |

### reporter

UT 纯本地断言（mock/纯函数）；ST 真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。
