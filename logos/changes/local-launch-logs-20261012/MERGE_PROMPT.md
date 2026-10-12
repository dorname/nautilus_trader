# 合并指令

## 变更提案
- 提案名称：local-launch-logs-20261012
- 提案目录：logos/changes/local-launch-logs-20261012/

## 提案内容

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


## 需要合并的 Delta 文件

### 1. deltas/test/core-S17-test-cases.md

- Delta 文件：`logos/changes/local-launch-logs-20261012/deltas/test/core-S17-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(local-launch-logs-20261012): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive local-launch-logs-20261012`。
