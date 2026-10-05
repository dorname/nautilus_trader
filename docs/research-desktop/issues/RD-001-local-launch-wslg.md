# RD-001 WSL2/WSLg 本地启动图形后端不稳定

- **类型**：bug / 环境
- **优先级**：P1
- **状态**：open
- **复现环境**：WSL2 + WSLg（`DISPLAY=:0`，`WAYLAND_DISPLAY=wayland-0`，`/mnt/wslg/runtime-dir`）
- **命令**：`cargo run -p nautilus-research-desktop`
- **相关代码**：`crates/research-desktop/src/main.rs`

## 现象

| 尝试 | 结果 |
|------|------|
| 默认（沙箱内） | 退出码 4：`Could not find wayland compositor` |
| 解除沙箱 + Wayland | 进程可存活，打开 `research-workspace/research.db`，但 X11 窗口树无标题为「研序」的窗口；`import -window root` 失败（Resource temporarily unavailable）；伴随 MESA/ZINK/EGL 告警 |
| `WINIT_UNIX_BACKEND=x11` | 退出码 101：`Library libxkbcommon-x11.so could not be loaded`（系统仅有 `libxkbcommon0`，未装 `libxkbcommon-x11-0`；sudo 需密码未能安装） |

启动失败时的用户提示（符合设计）：

```text
研究桌面启动失败：无法初始化图形后端（…）。
请确认运行环境具备图形显示（X11/Wayland；WSL2 需 WSLg 支持）。
```

## 期望

1. 在已启用 WSLg 的 WSL2 上，`cargo run -p nautilus-research-desktop` 弹出「研序 · 策略研究工作区」窗口并可操作。
2. 交付文档明确列出 Linux/WSL 运行时依赖（至少含 Wayland 或 X11 路径，以及 `libxkbcommon-x11` 等）。
3. 可选：无显示环境给出更具体的依赖缺失诊断（区分 compositor 缺失 vs xkb 库缺失）。

## 建议修复方向

- 文档：在 `core-03-desktop-delivery` / README 增补 WSL2 依赖清单与推荐环境变量。
- 工程：CI/本地 smoke 区分「无显示」与「有显示」；有显示环境再做窗口断言。
- 依赖：打包或安装说明中声明 `libxkbcommon-x11-0`（X11 回退路径）。

## 证据

- 终端输出（Wayland MESA 告警 / X11 xkb panic）见本次会话 `cargo run` 日志。
- 进程存活时 fd 含 `wayland-*` memfd 与 `research.db`，说明事件循环与桥接层已起，显示合成仍异常。
