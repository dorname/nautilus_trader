# 实现任务

## [code] 代码实现

- [x] `crates/research-desktop/src/app.rs`：`App::ui` 入口加首帧守卫（根矩形宽 <400 或高 <200 时跳帧重绘）
- [x] `crates/research-desktop/src/app.rs`：`render_body`/`render_canvas`/`render_chat` 等全部 `allocate_ui` 入参宽高 `max(0.0)` 钳制
- [x] `crates/research-desktop/tests/s15_desktop_l2.rs`：UT-S15-08 补窄窗输入（260×267）下布局纯函数不为负的断言
- [x] 限核回归桌面 6 套件 22 用例全绿（`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4`）
