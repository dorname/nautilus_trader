# 任务清单：local-launch-logs-20261012

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/test/core-S17-test-cases.md` — 新增 UT-S17-30（日志初始化与落盘）、UT-S17-31（启动脚本静态检查）

## [code] 代码实现

### 日志落盘
- [x] 新建 `crates/research-desktop/src/logging.rs`：`init_file_logger` + `logline`（std-only，stderr + 文件双写，`RESEARCH_DESKTOP_LOG_DIR` > `./logs`）
- [x] `lib.rs` 暴露 `pub mod logging`
- [x] `main.rs` milestone 接入落盘（main 开头 init）
- [x] `oup.rs` / `swarm_bridge.rs` / `swarm_ledger.rs` 共 11 处 eprintln! 收编为 logging::logline

### 启动脚本
- [x] 新建 `scripts/run-desktop.sh`（WSLg 自检修正 + libxkbcommon-x11 预检 + 构建 + mkdir logs + tee 控制台日志 + 参数透传，含 `--release` 开关）

### 测试与取证
- [x] UT-S17-30、UT-S17-31（OpenLogos reporter 入账）
- [x] 脚本实跑取证：WSLg 启动 GUI，窗口可枚举 + logs/ 两个日志文件非空

### 闭环
- [x] 本批覆盖用例：UT-S17-30、UT-S17-31（新增）；既有套件回归
