# RD-010 GUI 进程可起但窗口/截屏仍不可验收

- **类型**：bug / 环境
- **优先级**：P1
- **状态**：open
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
