## MODIFIED — 4. Agent 定义清单（manifest）

> **ADR 修订（agent-definitions，issue #22，2026-10-10）**：experiment-runner
> 不再设计为「持协调器工具的 agent」——实验执行由桌面侧直接调协调器
> （`DesktopBridge::submit_run`），LLM 只整理参数表单（`parse_run_params`
> 严格校验，拒绝任何结果数值字段），不持协调器工具、不产出数值、不触发
> 运行。octos stdio 侧无 agent/工具注册 API（见 #16/#20 能力子集说明），
> 工具白名单的约束力落在客户端派发校验（manifest definition_id）上。
