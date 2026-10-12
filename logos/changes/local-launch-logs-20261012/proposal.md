# 变更提案：本地源码启动脚本 + 日志落盘 logs 目录

> module: core | created: 2026-10-12

## 变更原因

本地调试 research-desktop 当前有两处摩擦（今日问答「本地怎么启动调试」沉淀为产品能力）：

1. **启动环境门槛高**：WSLg 下必须手工组合 `env -u WAYLAND_DISPLAY XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir DISPLAY=:0`，缺一项即静默退出或全黑（RD-001/RD-010 已证）；依赖 `libxkbcommon-x11-0` 缺失时 panic 才暴露。需要一键脚本封装正确环境。
2. **日志无落盘**：启动里程碑与运行时日志只走 stderr，终端关闭即丢失，事后排查（如「进程存活但无窗口」类问题）无凭据。需要日志写入指定 `logs/` 目录。

## 变更类型

代码级（新脚本 + 日志模块 + 测试用例 delta）

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：S17（desktop 启动可观测性口径延伸，不改场景定义）
- 影响的部署方案：无
- 影响的 API：无
- 影响的数据库：无
- 影响的测试用例：`logos/resources/test/core-S17-test-cases.md`（新增 UT-S17-30 / UT-S17-31）
- 影响的代码：
  - `scripts/run-desktop.sh`（新建）
  - `crates/research-desktop/src/logging.rs`（新建，std-only 零新依赖）
  - `crates/research-desktop/src/lib.rs`（`pub mod logging`）
  - `crates/research-desktop/src/main.rs`（milestone 接入落盘）
  - `crates/research-desktop/src/oup.rs`、`swarm_bridge.rs`、`swarm_ledger.rs`（11 处 eprintln! 收编为统一日志）
- 影响的文档：`docs/research-desktop/`（README 或启动说明，如已有对应文档节）

## 部署影响

- 是否需要部署：否
- 部署原因：本地开发工具与桌面端可观测性增强，无部署物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

**启动脚本** `scripts/run-desktop.sh`：
- WSLg 自检与修正：`/mnt/wslg/runtime-dir/wayland-0` 存在时自动修正 `XDG_RUNTIME_DIR`；强制去掉 `WAYLAND_DISPLAY`（winit 0.30 走 X11）；`DISPLAY` 缺省补 `:0`；非 WSLg 环境（如 Linux 桌面）原样透传不干预。
- 依赖预检：`libxkbcommon-x11-0` 缺失时给出中文 apt 安装提示后退出。
- 构建 + 启动：`cargo build -p nautilus-research-desktop`（`--release` 可选），`mkdir -p logs`，控制台全量输出 `tee` 到 `logs/console-<时间戳>.log`（兜底 panic 栈），参数透传。

**日志落盘** `logging.rs`（std-only，零新依赖，离线 verify 不受影响）：
- `init_file_logger()`：解析日志目录（环境变量 `RESEARCH_DESKTOP_LOG_DIR` > 默认 `./logs`），创建目录与 `research-desktop-<YYYYMMDD-HHMMSS>.log` 会话文件，返回路径。
- `logline(msg)`：`[HH:MM:SS.mmm] msg` 写 stderr + 追加会话文件（每行 open-append，低频日志无性能压力，无全局锁状态）。
- main.rs 的 `milestone` 与 oup/swarm_bridge/swarm_ledger 的 11 处 `eprintln!` 收编为 `logging::logline`，行为不变（仍 stderr），增加文件落盘。

## 验收口径

- UT-S17-30：init 创建目录与会话文件；logline 落盘内容含消息与时间戳（tmp 目录隔离）
- UT-S17-31：脚本静态检查——存在、`bash -n` 语法有效、含 WSLg 修正与 logs 目录逻辑
- 脚本实跑：WSLg 下启动 GUI 窗口可枚举，且 `logs/` 下两个日志文件非空
- `openlogos verify` Gate PASS（CPU 红线：CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1）
