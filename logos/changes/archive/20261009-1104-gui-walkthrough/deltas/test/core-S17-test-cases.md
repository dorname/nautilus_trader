## ADDED — S17 GUI 无头走查用例（gui-walkthrough 提案）

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
