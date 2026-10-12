#!/usr/bin/env bash
# 本地源码启动 research-desktop（研序 GUI）——封装 WSLg 环境修正与日志落盘。
# （local-launch-logs-20261012；环境修正口径沉淀自 RD-001 / RD-010）
#
# 用法：
#   scripts/run-desktop.sh              # debug 构建并启动
#   scripts/run-desktop.sh --release    # release 构建并启动
#   scripts/run-desktop.sh --no-build   # 跳过构建直接启动
#   RESEARCH_DESKTOP_LOG_DIR=/path      # 自定义日志目录（默认 ./logs）
#
# 日志：应用层里程碑落盘 logs/research-desktop-<时间戳>.log（UTC）；
#       本脚本再把控制台全量输出（含 panic 栈）tee 到 logs/console-<时间戳>.log。
set -euo pipefail

cd "$(dirname "$0")/.."

RELEASE=false
NO_BUILD=false
for arg in "$@"; do
    case "$arg" in
        --release)  RELEASE=true ;;
        --no-build) NO_BUILD=true ;;
        *) echo "未知参数：$arg（支持 --release / --no-build）" >&2; exit 2 ;;
    esac
done

# ── 显示环境修正（WSLg）───────────────────────────────────────────────
# /mnt/wslg/runtime-dir/wayland-0 存在即视为 WSLg：
#   - XDG_RUNTIME_DIR 必须指向它（默认 /run/user/0 无 wayland socket）
#   - 移除 WAYLAND_DISPLAY 强制 X11（winit 0.30 已移除 WINIT_UNIX_BACKEND）
#   - DISPLAY 缺省补 :0
if [ -S /mnt/wslg/runtime-dir/wayland-0 ]; then
    if [ "${XDG_RUNTIME_DIR:-}" != "/mnt/wslg/runtime-dir" ]; then
        echo "[run-desktop] 修正 XDG_RUNTIME_DIR → /mnt/wslg/runtime-dir（原：${XDG_RUNTIME_DIR:-未设置}）"
        export XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir
    fi
    if [ -n "${WAYLAND_DISPLAY:-}" ]; then
        echo "[run-desktop] 移除 WAYLAND_DISPLAY（winit 0.30 走 X11）"
        unset WAYLAND_DISPLAY
    fi
    if [ -z "${DISPLAY:-}" ]; then
        echo "[run-desktop] 补 DISPLAY=:0"
        export DISPLAY=:0
    fi
else
    echo "[run-desktop] 非 WSLg 环境（无 /mnt/wslg/runtime-dir/wayland-0），显示变量不干预"
fi

# ── 依赖预检：libxkbcommon-x11（缺失时 winit X11 路径 panic）───────────
# 注：先落变量再 grep——pipefail 下 `ldconfig -p | grep -q` 会因 grep 提前
# 退出让 ldconfig 吃 SIGPIPE(141)，管道整体非零造成误报。
LDCACHE="$(ldconfig -p 2>/dev/null || true)"
if ! grep -q "libxkbcommon-x11" <<< "$LDCACHE"; then
    echo "[run-desktop] 错误：未找到 libxkbcommon-x11（X11 键盘支持库）" >&2
    echo "           安装：sudo apt install libxkbcommon-x11-0" >&2
    exit 3
fi

# ── 构建 ──────────────────────────────────────────────────────────────
PROFILE_DIR=debug
BUILD_ARGS=(-p nautilus-research-desktop)
if [ "$RELEASE" = true ]; then
    PROFILE_DIR=release
    BUILD_ARGS+=(--release)
fi
if [ "$NO_BUILD" = false ]; then
    echo "[run-desktop] 构建（$PROFILE_DIR）…"
    cargo build "${BUILD_ARGS[@]}"
fi

BIN="target/$PROFILE_DIR/research-desktop"
if [ ! -x "$BIN" ]; then
    echo "[run-desktop] 错误：$BIN 不存在或不可执行（去掉 --no-build 重试）" >&2
    exit 3
fi

# ── 日志目录与启动（控制台全量 tee 兜底 panic 栈）──────────────────────
LOG_DIR="${RESEARCH_DESKTOP_LOG_DIR:-logs}"
mkdir -p "$LOG_DIR"
TS="$(date -u +%Y%m%d-%H%M%S)"
CONSOLE_LOG="$LOG_DIR/console-$TS.log"
echo "[run-desktop] 启动 $BIN（控制台日志：$CONSOLE_LOG）"
"$BIN" 2>&1 | tee "$CONSOLE_LOG"
