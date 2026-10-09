## ADDED — 桌面 GUI 无头走查基础用例（gui-walkthrough 提案）

# 桌面 GUI 无头走查基础用例（gui-walkthrough 提案，crates/research-desktop tests/gui_walkthrough.rs，reporter 入账）

egui_kittest 无头渲染与结构断言（不调 LLM）：导航可达性、空态门控与
空闲任务条口径。nav_item 自绘导航项补 widget_info 后对 AccessKit 可见。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-GUI-01 | S15-AC-02 | 无头遍历侧栏 11 路由导航 | 每页渲染不 panic 且路由切换生效；需求页「确认需求」按钮以 Button 角色可定位；截图落盘 |
| UT-GUI-02 | S15-AC-02 | 空态（无实验）打开回测实验页 | 出现「运行合成实验」空态引导按钮；「运行参数（协调器）」折叠区不渲染（UI 既有门控口径） |
| UT-GUI-03 | S15-AC-02 | 空闲态（无任何任务）渲染 | 无「多 Agent 分析」任务行；无任务条「取消」按钮 |

### reporter
纯本地渲染断言，无外部调用；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。
