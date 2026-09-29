## ADDED — 场景编号与调用索引
既有验收文档已使用 S10，而索引计数器遗留为1。本提案核对 resources 和 changes 后预留 S11～S15，并将 logos-project.yaml 的 next_id 更新为16；取消提案也不回收编号。基线 ST-01～08 不重命名。
| 场景 | 文件 | 主要契约 |
|---|---|---|
| S11 | core-S11-data-snapshot.md | ImportData、GetTask、CancelTask、ListSnapshots |
| S12 | core-S12-universe.md | PreviewUniverse、SaveUniverse、QueryRows |
| S13 | core-S13-research-run.md | SubmitRun、GetTask、CancelTask |
| S14 | core-S14-compare.md | CompareRuns、QueryRows |
| S15 | core-S15-trade-plan.md | GeneratePlan、ExportPlan、SaveManualNote |
所有页面统一通过协调器调用；任务查询恢复和分页属于主消息的查询子流程。API 文件记录每个操作的场景来源，不另设无场景的业务接口。

## ADDED — S16 原型开发扩展
新增S16“编写策略并送入研究”，预留编号后next_id=17。原型内存交互时序见 `prd/2-product-design/2-page-design/core-04-strategy-development-design.md`；本轮不新增生产服务API，不暗示已实现用户策略执行。
