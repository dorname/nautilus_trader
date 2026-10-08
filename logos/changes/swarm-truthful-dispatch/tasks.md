# 实现任务

## [delta] 规格变更
- [x] 产出 `deltas/test/core-S17-test-cases.md` — 新增 Swarm 真实终态用例：ST-S17-19（并发派发等 turn/completed，三契约全 Succeeded 且 output 非空）、ST-S17-20（派发后首轮轮询保持 Active，不得假 Completed/Succeeded）、UT-S17-18（终态归约纯函数：空集/全成功/含失败/含取消的判定矩阵，失败不得吞为成功）
- [x] 产出 `deltas/test/core-S18-test-cases.md` — 新增 Pipeline 注入用例：ST-S18-16（串行执行：stage2 开始晚于 stage1 完成；stage2 final_prompt 含 stage1 输出且不含 `{pipeline_input}` 字面量；两 stage 均 Succeeded 且 output 非空）

## [code] 代码实现

- [ ] 实现代码变更