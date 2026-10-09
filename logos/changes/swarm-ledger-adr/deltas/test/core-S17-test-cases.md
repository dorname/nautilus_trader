## ADDED — swarm 账本与聚合校验收尾用例（swarm-ledger-adr 提案，issue #23/#25）

# swarm 账本与聚合校验收尾用例（swarm-ledger-adr 提案，crates/research-desktop tests，reporter 入账）

崩溃恢复诚实口径：GUI/进程重启后，账本中未完成派发一律标记
「中断（进程重启）」——octos 子进程已随旧进程死亡，不得假装仍在运行；
聚合校验（M4.3 validator 语义）：三路产物全 Succeeded 且 output 非空才
允许挂载，否则阻断。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-25 | S17-AC-01 | 无 LLM：SwarmLedger 写入派发 → 终态更新；模拟重启（重开账本）读未完成项并标记中断 | 派发记录含 dispatch_id/契约/turn_id/状态；终态更新后可查；重启后未完成派发全部转为「中断（进程重启）」且不残留 Active |
| UT-S17-26 | S17-AC-01 | aggregate_valid 判定矩阵：全 Succeeded 有 output / 含 Failed / 含 Cancelled / output 空串 / 空结果集 | 仅全 Succeeded 且 output 非空返回 true；其余一律 false（阻断挂载） |

### reporter
纯本地账本/纯函数断言，无外部调用；结果经共享 reporter 写入
test-results.jsonl，fail 含 error，skip 不可计为通过。
