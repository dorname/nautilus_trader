## ADDED — 本地启动脚本与日志落盘用例（local-launch-logs-20261012，crates/research-desktop + scripts/，reporter 入账）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S17-30 | S17-AC-01 | 无 LLM：`logging::init_file_logger` 指定 tmp 日志目录初始化后，`logline` 写消息 | 目录自动创建；生成 `research-desktop-<时间戳>.log` 会话文件；logline 后文件内容含该消息与 `[HH:MM:SS]` 时间戳；stderr 行为不变 |
| UT-S17-31 | S17-AC-01 | 无 LLM：`scripts/run-desktop.sh` 静态检查 | 脚本存在且可执行；`bash -n` 语法校验通过；内容含 WSLg 修正（XDG_RUNTIME_DIR / 去 WAYLAND_DISPLAY）、`logs` 目录创建与 tee 落盘逻辑、libxkbcommon-x11 预检 |

### reporter

纯本地断言（tmp 目录隔离，无外部调用）；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。
