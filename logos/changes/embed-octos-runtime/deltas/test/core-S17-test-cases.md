# Delta: core-S17-test-cases.md

## ADDED — Server 保障与 LLM 配置用例（embed-octos-runtime，crates/research-desktop tests/，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-28 | S17-AC-01 | 无 LLM：`backend_ensure` 三级解析（内嵌资源 → `~/.octos/bin` → PATH）与 `REQUIRED_OCTOS_RELEASE` 版本 pin | 三级顺序正确且记录实际来源；非内嵌来源版本不匹配时返回中文诊断错误（含安装指引），不静默继续；三级全缺失时给出引导错误，spawn 被拒绝 |
| UT-S17-29 | S17-AC-01 | 无 LLM：`AgentSessionConfig::desktop()` profile 复制失败（源缺失/目标不可写）不再 `let _` 吞错 | 复制失败显式报错并给出引导文案（提示 RESEARCH_OCTOS_PROFILE 与 ~/.octos 两个来源）；profile 档位发现能列出 `data_dir/profiles/*.json` 的全部档位 ID；单档/空目录/不可读目录均不 panic |

### reporter

纯本地解析/校验断言，无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。

## ADDED — 产物文件型契约与工作区回流用例（embed-octos-runtime，crates/research-desktop tests/，真实 LLM，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| ST-S17-23 | S17 多 Agent 分析闭环 | 真实 LLM：确认需求派发产物文件型三路契约（每路在托管工作区写分析文件）；终态后桌面从 `<instance>/profiles/<profile>/data/users/<user>/workspace/` 回流 | 三路 turn 真实终态；托管工作区出现契约声明的产物文件且非空白；产物文本挂载到需求版本 R_n；文件缺失/空白的一路按失败处理且不挂载冒充文本；`aggregate_valid` 聚合口径不变 |

### reporter

真实 LLM 调用遵循 RESEARCH_OCTOS_PROFILE 配置选择（与
wire_real_llm 同口径）；结果经共享 reporter 写入 test-results.jsonl，
fail 含 error，skip 不可计为通过。
