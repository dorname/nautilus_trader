# 实现任务

## [delta] 规格变更
- [x] deltas/prd：core-06 决策表与约束第 5 条 ADR 修订（#25）
- [x] deltas/test/core-S17-test-cases.md：注册 UT-S17-25 / UT-S17-26

## [code] 代码实现

- [x] research-desktop 加 redb 依赖；swarm_ledger 模块（record_dispatch/record_terminal/unfinished/mark_unfinished_interrupted/recover_on_startup）
- [x] SwarmWorker dispatch/终态（正常/取消/超时/轮询错误四分支）写账本（失败降级不阻断）
- [x] GUI attach_bridge 后账本重建：未完成派发标记「中断（进程重启）」并 tell/record
- [x] aggregate_valid 纯函数提取（聚合校验，poll_swarm_task 改用，#23）
- [x] UT-S17-25 / UT-S17-26 入账 reporter 并通过
