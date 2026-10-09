## ADDED — S19 GUI 无头走查用例（gui-walkthrough 提案）

# S19 GUI 无头走查用例（gui-walkthrough 提案，crates/research-desktop tests/gui_walkthrough.rs，reporter 入账）

S19 整理参数的 GUI 旅程口径：LLM 只填表单，提交必须由用户二次确认；
走查沿用 UI 既有门控（运行参数折叠区仅在已有实验时渲染），
前置链路（设计/策略版本/合成实验）均为本地操作，不经 LLM。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-GUI-02 | S19-AC-01 | 无头完成 保存设计版本 → 检查草稿 → 保存版本 → 运行合成实验 → 展开「运行参数（协调器）」→ 点击「AI 整理参数」（真实 LLM） | 任务条出现「整理实验参数」LLM 任务行；终态产出「运行参数已填入」卡（不得是「参数整理失败」）；运行表单 start/end 非空且较缺省变化、fast<slow、top_k>0；停留回测实验页等待人工提交（未自动运行）；截图落盘 |

### reporter
真实 LLM 调用产生 API 费用；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。
