# 任务清单：fix-reallm-stability-20261012

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/test/core-S18-test-cases.md` — 新增 UT-S18-19（completed 后宽限窗口收割迟到的 assistant_persisted）、UT-S18-20（宽限期满仍空返回空文本语义）

## [code] 代码实现

### #36 渲染 SIGSEGV 修复
- [ ] `gui_walkthrough.rs`：`finish(w)` 改为 `std::mem::forget(w)` + `walk_finished()`，注释注明根因与同源修法
- [ ] 渲染模式（无 `--test-threads=1`）单跑 `st_gui_journey_real_llm` 实测不再 SIGSEGV（WSLg 取证）

### #37 偶发空产物修复
- [ ] `oup.rs`：实现宽限收割（独立函数，`turn_wait_completed` 在 completed 到达但文本空时进入 ≤3s 宽限窗口继续收割 assistant_persisted + tool events）；补 `recv_timeout`/`Disconnected` 路径遗漏的 `parse_tool_event`
- [ ] `swarm_bridge.rs`：空产物失败消息细化为可区分分类（completed 已到但 persisted 宽限内未到达/模型空回复）
- [ ] 测试代码：UT-S18-19、UT-S18-20（channel 注入 mock 通知流，含 OpenLogos reporter）；回归 ST-S18-16 / st_m3_01 真实 LLM 路径

### 闭环
- [ ] 本批覆盖用例：UT-S18-19、UT-S18-20（新增）+ ST-GUI-01/02、ST-S18-16、ST-M3-01（回归）；reporter 入账 test-results.jsonl
