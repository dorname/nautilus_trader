# 实现任务

## [delta] 规格变更
- [x] 无规格语义变更（测试资产补强，[manual] GUI 旅程口径保持不变，无需 delta）

## [code] 代码实现

- [x] research-desktop 增加 dev-dependencies：egui_kittest 0.36.2（snapshot/wgpu）+ image 0.25.6（png）
- [x] nav_item 自绘导航补 widget_info（AccessKit 无障碍元信息，不改变视觉与交互）
- [x] app.rs 增加 #[doc(hidden)] 测试访问器：test_current_route / test_requirement_swarm_analysis / test_run_form
- [x] 新增 tests/gui_walkthrough.rs：UT-GUI-01 全路由渲染与按钮可达、UT-GUI-02 空态门控（无实验时运行参数区不渲染）、UT-GUI-03 空闲无任务条（均无 LLM）
- [x] ST-GUI-01：确认需求 → 三路进度行 → 完成卡 → swarm_analysis 挂载（真实 LLM）
- [x] ST-GUI-02：设计/策略版本 → 合成实验 → AI 整理参数 → 表单填入 → 停留待二次确认（真实 LLM）
- [x] 全部用例接入 OpenLogos reporter 并 `cargo test -p nautilus-research-desktop --test gui_walkthrough` 通过（4/4，含 wgpu 截图证据 target/gui-walkthrough/）
