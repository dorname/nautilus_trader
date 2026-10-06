# 实现任务

## [delta] 规格变更
- [x] 产出 delta 文件到 `deltas/test/` — `core-S15-test-cases.md` 表格新增 ST-S15-06（无头整页几何回归：双窗口档、画布/对话栏分区、标题单行宽度、指标卡纵向堆叠、无越界绘制）

## [code] 代码实现
- [ ] F1：`render_body` 改显式矩形切列 + `new_child(Layout::top_down(Align::Min))`（app.rs）
- [ ] F2：审计 11 页 `horizontal*` 内 `allocate_ui`/`Frame::show` 同类点并改显式 top_down（含指标卡 app.rs:1261）
- [ ] F3：提取 `render_root` + 实现 ST-S15-06 无头整页几何回归测试（app.rs `#[cfg(test)]`）
- [ ] RD-008 issue 文档状态置 fixed
