# 实现任务

## [code] 代码实现

- [x] `crates/research-desktop/src/icons.rs`（新）：原型 10 组 SVG path 转写为 egui 点列表常量 + `for_route(Route)` 分发 + `paint_icon(painter, rect, kind, color)`
- [x] `crates/research-desktop/src/app.rs`：nav_item 加 16px 图标（文本左移 24px；激活态 ACCENT_TEXT、非激活 MUTED）
- [x] `crates/research-desktop/src/app.rs`：overview hero 补 110px 双环装饰（圆+双椭圆 40°/−40°+中心点，窄窗降透明度）+ 渐变底
- [x] `crates/research-desktop/src/app.rs`：empty_panel 顶部加 30px flow 图标（#3D5C4A）
- [x] ~~实验页净值曲线~~（缩减：桌面端无逐日净值数据源，需域层 API 扩展，另立提案）
- [x] `crates/research-desktop/tests/s15_desktop_l2.rs`：UT-S15-08 补图标分发全覆盖/路径点非空/装饰色值断言
- [x] 限核回归桌面 6 套件 22 用例全绿（`CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4`）
