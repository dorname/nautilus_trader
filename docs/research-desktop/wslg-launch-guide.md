# research-desktop WSLg 启动指南与排障

> 关联 issue：#32（RD-032 进程存活但「无窗口」）、#1（RD-001 启动不稳定）。
> 2026-10-09 实测结论：**应用窗口经原生 Wayland 正常创建并出帧**，
> 「无窗口」是诊断口径错误（xwininfo 只见 X11/XWayland 窗口，看不到
> Wayland 窗口），非应用缺陷。

## 推荐启动

```bash
cargo run -p nautilus-research-desktop
# 或（软件渲染，无 GPU 直通时）
LIBGL_ALWAYS_SOFTWARE=1 ./target/debug/research-desktop
```

WSL2 需 WSLg 组件（`WAYLAND_DISPLAY=wayland-0`、`DISPLAY=:0` 均存在即正常）。
窗口标题：「研序 · 策略研究工作区」，在 Windows 侧桌面可见。

## 启动日志与退出码

启动里程碑日志输出到 stderr（`[research-desktop] ...`）：

1. 环境变量快照（DISPLAY / WAYLAND_DISPLAY / 后端相关）
2. 进入 `eframe::run_native`
3. 窗口与图形后端已创建（CreationContext 就绪）
4. 字体加载完成 → 桥接完成 → 首帧渲染到达

| 退出码 | 含义 |
|--------|------|
| 0 | 正常退出（关闭窗口） |
| 4 | 图形后端初始化失败（run_native 返回错误） |
| 5 | 首帧看门狗超时（15s 未见首帧，视为窗口创建失败） |

## 排障

1. **看日志停在哪一个里程碑**：停在「进入 run_native」之后无「CreationContext
   就绪」，是 winit/wgpu 初始化问题；有「CreationContext 就绪」但无「首帧
   渲染到达」，是事件循环/渲染问题。
2. **不要用 xwininfo 判断窗口是否存在**（只见 X11 窗口）。Wayland 侧验证：
   `WAYLAND_DEBUG=1 ./target/debug/research-desktop 2>&1 | grep xdg_toplevel`，
   应见 `set_title("研序 · 策略研究工作区")` 与 configure/ack_configure 往返。
3. WSLg 组件异常：`wsl --shutdown`（Windows 侧）后重新进入。
4. 可选后端开关：`WINIT_UNIX_BACKEND=x11`（强制 XWayland，此时窗口
   才会出现在 xwininfo 树中）、`WGPU_BACKEND=gl`。

## 窄视口（≤900px）对照

原型 `@media(max-width:900px)` 收侧栏为 65px 图标栏；桌面侧同口径
（`layout.rs` Slim 计划，`SIDEBAR_W_SLIM=65`）。对照证据：
`docs/research-desktop/screenshots/ut-gui-04-narrow.png`（UT-GUI-04 自动
生成，880px 无头渲染）。
