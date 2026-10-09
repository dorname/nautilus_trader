## MODIFIED — 1. 设计决策总览

> **ADR 修订（swarm-ledger-adr，issue #25，2026-10-09）**：多 Agent 编排的
> 权威口径由「复用 octos-swarm 的 `Swarm::dispatch`」修订为
> 「**客户端侧并行 turn（SwarmBridge 经 OUP stdio）**」。理由：
> `octos-swarm` 不作为库公开发布（crates.io 无此 crate，octos 仅以 CLI
> 分发），research-desktop 作为 OUP stdio 客户端无法进程内复用；客户端
> 编排已具备 Parallel/Pipeline 拓扑、真实 turn 终态归约（不得吞失败为
> 成功）、turn/interrupt 真实中断、redb 账本持久化与聚合校验，能力等价
> 于原承诺。redb 账本主体由「octos 自带」修订为「research-desktop 自建」
> （`swarm_ledger` 模块，`{agent_data_dir}/swarm/swarm-ledger.redb`）。
