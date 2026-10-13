## MODIFIED — 真实 LLM 接线用例引言段（原文「真实 LLM（moonshot-coding profile）经 OUP 全链路」段）

真实 LLM（moonshot-coding profile）经 OUP 全链路：session/open → turn/start →
事件流（message/delta、projection/envelope）→ turn/completed 终态。回复文本的
**持久化来源**只从 `projection/envelope` 中 `type=assistant_persisted` 的持久化
产物提取（RD-018：落盘与终态文本禁止拼接 message/delta）；自
fix-issues-40-41-20261013（issue #41）起，展示层可消费 message/delta 做实时
预览，预览随 turn/completed 由持久化产物替换落定，二者不一致以持久化产物为准；
demo_task 演示路径整体移除（RD-008/RD-030）。

## ADDED — 流式预览回归用例（fix-issues-40-41-20261013 / issue #41）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S17-33 | S17-AC-01 | 无 LLM：message/delta 归集纯函数——按 turn_id 累积增量帧；异 turn_id 帧混入；空增量帧；turn/completed 后终态替换 | 累积文本按到达顺序拼接；异 turn_id delta 不混入；空增量忽略；终态文本以 assistant_persisted 替换而非与 delta 拼接 |
| UT-S17-34 | S17-AC-01 | 无 LLM：源码静态检查——message/delta 仅流入展示预览通道 | 静态扫描确认持久化/落盘/版本化路径不含 delta 拼接，回复文本持久化来源仍只是 assistant_persisted；预览状态在 turn 终态后被替换清空 |
