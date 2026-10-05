# 实现任务

## [code] 代码实现

- [x] `crates/research-desktop/src/app.rs`：品牌 mark 改 painter 145° 渐变 Mesh + 14px 辉光环（#4ADE80→#16A34A + rgba(34,197,94,.35)）
- [x] `crates/research-desktop/src/app.rs`：nav_item 激活指示条外加 10px 辉光（三层透明度递减）
- [x] `crates/research-desktop/src/layout.rs`：`LayoutPlan` 增 `Slim` 档（≤900：侧栏 65）；`plan_for_width` 三段分流
- [x] `crates/research-desktop/src/app.rs`：Slim 档侧栏只显图标（隐藏品牌名/section-label/计数/新建按钮文字）
- [x] `crates/research-desktop/src/theme.rs` + 字体资产：内嵌 CJK 字体兜底（`include_bytes!`），消除字体缺失告警
- [x] `crates/research-desktop/src/app.rs`：footer 改「● 本地研究桌面 · 合成/导入样本可追溯 · 结果绑定版本与快照」
- [x] `crates/research-desktop/tests/s15_desktop_l2.rs`：UT-S15-08 补 Slim 断点/辉光色值/内嵌字体/footer 文案断言
- [x] 限核回归 22 用例全绿 + 重建实跑目检
