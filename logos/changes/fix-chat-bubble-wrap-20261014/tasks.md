# 实现任务

## [delta] 规格变更
- [x] 产出 delta 文件到 `deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md` — 新增「对话气泡排版约束（issue #40 跟进）」章节
- [x] 产出 delta 文件到 `deltas/test/core-S15-test-cases.md` — 新增 UT-S15-14（长用户消息气泡换行不越界）回归用例

## [code] 代码实现
- [ ] 修复 `crates/research-desktop/src/app.rs` render_message 用户分支：气泡与「你」标签改用 top_down(Align::Max) 右对齐，换行宽度 min(360, 对话栏可用宽)，显式启用换行
- [ ] 在 `crates/research-desktop/tests/gui_walkthrough.rs` 实现 UT-S15-14 无头渲染回归用例（含 OpenLogos reporter 入账）
- [ ] 本地跑通 research-desktop 测试（CPU 保护三件套：nice -n 19 + -j 2 + --test-threads=1）
