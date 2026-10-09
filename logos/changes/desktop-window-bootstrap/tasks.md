# 实现任务

## [delta] 规格变更
- [x] deltas/test/core-S15-test-cases.md：注册 UT-GUI-04（窄视口布局对照）

## [code] 代码实现

- [x] main.rs 启动里程碑日志（环境快照/run_native/CreationContext/字体/桥接/首帧，stderr）
- [x] 首帧看门狗（15s 无首帧 → 诊断 + 退出码 5；FirstFrameProbe 包装）
- [x] WSLg 实测定位：窗口经原生 Wayland 正常创建（xdg_toplevel set_title 有证），#32「无窗口」为 xwininfo 诊断口径错误（只见 X11）；应用无缺陷，不改为默认 X11
- [x] docs/research-desktop/wslg-launch-guide.md（推荐参数/里程碑日志/退出码表/排障/xwininfo 误区/WAYLAND_DEBUG 验证法）
- [x] UT-GUI-04：880px slim 图标栏断言（导航项宽 <100px，对照 1440px >150px）+ 截图 docs/research-desktop/screenshots/ut-gui-04-narrow.png（#7）
