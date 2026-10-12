# RD-010 GUI 进程可起但窗口/截屏仍不可验收

- **类型**：bug / 环境
- **优先级**：P1
- **状态**：resolved（2026-10-12 复测关闭：窗口可枚举 + 截屏非黑实证）
- **关联**：RD-001（本条为 2026-10-11 复测续篇）
- **GitHub**：[#35](https://github.com/dorname/nautilus_trader/issues/35)
- **命令**：`research-desktop`（`cargo run -p nautilus-research-desktop`）

## 现象（2026-10-11）

| 尝试 | 结果 |
|------|------|
| Agent 沙箱内默认环境 | 退出：`Could not find wayland compositor`（`XDG_RUNTIME_DIR=/run/user/0`，无 wayland socket） |
| `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir` + Wayland | 进程存活；fd 含 `wayland-cursor` memfd 与 `research-workspace/research.db` |
| `WINIT_UNIX_BACKEND=x11`（`libxkbcommon-x11-0` 已安装） | 进程存活；打开同一 `research.db` |
| `scrot` 根窗口截屏 | 产出全黑 PNG（`docs/research-desktop/screenshots/local-loop-2026-10-11.png`） |
| `xwininfo` / `wmctrl` 枚举「研序」窗口 | 不可用或无客户端列表 |

## 与 RD-001 差异

- **已缓解**：系统已装 `libxkbcommon-x11-0`；在正确 `XDG_RUNTIME_DIR` 下 Wayland/X11 均可让进程存活并打开工作区 DB。
- **仍阻塞**：无法用截屏/窗口树证明 UI 可见可操作；全黑截屏表明合成/渲染路径仍异常。CLI 闭环不能替代 GUI 验收。

## 期望

1. WSLg 下弹出「研序 · 策略研究工作区」且截屏可见非黑帧。
2. 文档写明：`XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir`（勿用 `/run/user/0`）、Agent 沙箱对 Wayland socket 的限制。
3. 可选：无显示环境给出 compositor / runtime-dir 诊断。

## 建议修复方向

- 启动日志打印实际使用的 `XDG_RUNTIME_DIR` 与 backend（wayland/x11）。
- smoke：有显示时断言窗口标题或非黑离屏帧；无显示时 skip 并写明原因。

## 2026-10-12 复测：关闭结论

**启动命令（X11 路径，可复现）**：

```bash
env -u WAYLAND_DISPLAY XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir DISPLAY=:0 \
  ./target/debug/research-desktop
```

注：winit 0.30 已移除 `WINIT_UNIX_BACKEND` 环境变量，强制 X11 需**去掉 `WAYLAND_DISPLAY`** 而非设变量。

| 期望 | 结果 |
|------|------|
| 1. 窗口弹出且截屏非黑 | ✅ 窗口可枚举、窗口像素非黑（证据如下） |
| 2. 文档写明 `XDG_RUNTIME_DIR` | ✅ 本节即归档；`/run/user/0` 无 wayland socket，必须用 `/mnt/wslg/runtime-dir` |
| 3. 可选诊断 | 留作后续可选提案（启动日志打印 backend、smoke 非黑断言），非关闭阻塞项 |

### 证据

- **窗口可枚举**：`xwininfo -root -tree` 出现 `0xc00002 "研序 · 策略研究工作区" 1440x900+166+289`（2026-10-11 时枚举不到，当时进程可能处于 Wayland 路径或未完成 map）。
- **截屏非黑**：python-xlib 对窗口 drawable 直接 `get_image`（非根窗口），1440×900 全幅平均亮度 **22.1/255**、非黑像素占比 **45.3%**（深色主题 UI 的正常分布）。证据图：[`../screenshots/local-loop-2026-10-12.png`](../screenshots/local-loop-2026-10-12.png)（界面完整：侧边栏 / 项目概览 / 研究助手面板）。
- **scrot 全黑根因**：WSLg（Weston + XWayland）下根窗口合成不返回客户端像素，`scrot` 抓根窗口必得全黑；验收须对**窗口本身** `get_image`，或枚举窗口标题断言存在。
- **资源占用**：静默 CPU 三次采样均 **0.9%**（启动预热峰值约 12%，一次性），满足静默零帧红线。

### 验收方法备忘（后续 smoke 设计输入）

1. `xwininfo -root -tree | grep "研序 · 策略研究工作区"` 断言窗口存在；
2. python-xlib `get_image` 抓该窗口，断言非黑像素占比 > 5%；
3. 无显示环境（无 `/mnt/wslg/runtime-dir/wayland-0` 且无 `DISPLAY`）skip 并写明原因。

## 2026-10-12 下午：误 reopen 纠正 + X11 复测

| 项 | 结果 |
|----|------|
| 误 reopen 原因 | 保留 `WAYLAND_DISPLAY` + `scrot` 抓**根窗口**（WSLg 下必得全黑），违反本条关闭证据口径 |
| 正确启动 | `env -u WAYLAND_DISPLAY XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir DISPLAY=:0 ./target/debug/research-desktop` |
| `xwininfo` | `0xc00002 "研序 · 策略研究工作区" 1440x900` |
| 窗口 `get_image` | avg **21.9/255**，非黑 **98.3%**，`dark_ratio=0.017` |
| 证据 | [`../screenshots/local-loop-2026-10-12-pm-x11.png`](../screenshots/local-loop-2026-10-12-pm-x11.png) |
| 处置 | **重新关闭** [#35](https://github.com/dorname/nautilus_trader/issues/35)（评论 15:36） |
