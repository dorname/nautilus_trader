# 变更提案：gui-walkthrough

> module: core | created: 2026-10-08

## 变更原因
多 Agent 场景入口（swarm-scene-entry：issue #26 确认需求派发 S17 三路分析、
issue #15 S19 LLM 整理参数接真实协调器）已通过 bridge/worker 层 ST 验收
（verify 124/124），但 **GUI 层旅程尚无自动化走查**：按钮可见性、任务条三路
进度行、完成卡、产物挂载后需求文档展示、运行页「AI 整理参数」填表与
二次确认停留等只经人工目检。issue #1/#32 表明当前 WSL2/WSLg 环境无可见
窗口，人工走查不可用，需要无头（headless）GUI 走查补齐证据。

## 变更类型
代码级（验证补强：仅新增 dev-dependency 与集成测试，不改任何运行时行为与规格语义）

## 变更范围
- 影响的需求文档：无（验收口径不变，仅把既有 [manual] GUI 旅程中可自动化部分落地为测试）
- 影响的功能规格：无
- 影响的业务场景：core-S17（需求确认→三路分析→产物挂载）、core-S19（AI 整理参数→二次确认）的 GUI 落地验证
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：新增 crates/research-desktop/tests/gui_walkthrough.rs（UT 渲染层 + ST 真实 LLM 旅程）

## 部署影响
- 是否需要部署：否
- 部署原因：纯测试资产与开发依赖，无运行时交付物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false            # 本次是否触及界面（GUI 项目才有意义）
design_system_mode: generated   # generated | fallback（fallback 时须填 design_system_fallback_reason）
design_system_fallback_reason: ""
pages: []                   # 每项 {id, prototype: core-NN-<slug>.html, description}
```

## 变更概述
为 research-desktop 引入 `egui_kittest`（与 egui 0.36.2 同版本）作为
dev-dependency，基于 AccessKit 无头驱动真实 `ResearchApp::render_root`
（延续 ST-S15-06 的 `Context::run_ui` 无头先例），新增
`tests/gui_walkthrough.rs`：

1. UT 层（无 LLM）：渲染关键路由页不 panic；「确认需求」「AI 整理参数」
   按钮在 AccessKit 树中可见且可点击；未确认需求时整理参数给出前置提示。
2. ST 层（真实 LLM，串行）：点击「确认需求」→ 任务条出现三路进度行 →
   终态完成卡 → 工作区需求版本文件写入 swarm_analysis 摘要；
   点击「AI 整理参数」→ 运行表单被填入合法参数 → 停留运行页等待二次
   确认（不自动提交）。
