# 变更提案：真实 LLM 稳定性修复（#36 渲染 SIGSEGV / #37 偶发空产物）

> module: core | created: 2026-10-12

## 变更原因

2026-10-12 全功能/全流程/多 Agent 业务测试发现两个真实 LLM 路径稳定性问题（均已提 GitHub issue 并完成根因诊断）：

- **[#36](https://github.com/dorname/nautilus_trader/issues/36)**：`gui_walkthrough` ST-GUI-01/02 真实 LLM 旅程在**渲染启用**时 SIGSEGV。本轮复现矩阵锁定：串行无渲染（`--test-threads=1`）235.75s 通过、渲染启用（`--exact`）必崩。根因：wgpu 软渲染（llvmpipe/LLVM JIT）线程在 `finish(w)` 的 `drop(w)` 及进程收尾阶段触发 glibc `nptl_deallocate_tsd` TLS 析构竞态；现有 `walk_finished` 的 exit(0) 守卫注册于 `drop(w)` **之后**，兜不住 drop 路径的提前崩溃。
- **[#37](https://github.com/dorname/nautilus_trader/issues/37)**：`st_m3_01_pipeline_dispatch` 偶发 design stage `assistant_persisted` 为空（全量跑失败一次、单测复跑通过）。根因主嫌疑：`submit_and_wait`/`turn_wait_completed` 等到 `turn/completed` 即返回，服务端先推 completed、后落盘再推 `assistant_persisted` 时，迟到的 persisted 通知被错过（时序竞态，与同族 turn_id 回显不稳定问题同源）。

## 变更类型

代码级修复（含测试用例 delta：新增 UT 用例）

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：S18（Pipeline 接力稳定性口径细化，不改场景定义）
- 影响的部署方案：无
- 影响的 API：无
- 影响的数据库：无
- 影响的测试用例：`logos/resources/test/core-S18-test-cases.md`（新增 UT-S18-19 / UT-S18-20）
- 影响的代码：
  - `crates/research-desktop/tests/gui_walkthrough.rs`（#36：`finish` 泄漏 Harness 替代 drop）
  - `crates/research-desktop/src/oup.rs`（#37：completed 后宽限收割 + 补 recv_timeout/Disconnected 路径遗漏的 tool event 收集）
  - `crates/research-desktop/src/swarm_bridge.rs`（#37：空产物失败分类消息细化）
- 影响的文档：`docs/research-desktop/issues/RD-011*.md / RD-012*.md`（修复回填，如并行会话已建）

## 部署影响

- 是否需要部署：否
- 部署原因：桌面端测试基建与客户端收割逻辑修复，无部署物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

**#36 修复**：`gui_walkthrough.rs` 的 `finish(w)` 将 `drop(w)` 改为 `std::mem::forget(w)`——泄漏 Harness/wgpu 资源（进程随即经 `walk_finished` 异步 exit(0) 回收），彻底消除渲染后端 drop 路径的 TLS 析构竞态触发面；注释说明与 issue_closure.rs 泄漏线程修法同源。

**#37 修复**：`oup.rs` 新增宽限收割——`turn_wait_completed` 在 `turn/completed` 到达但 `assistant_text` 为空时，不立即返回，进入宽限窗口（3s）继续泵 `rx`/`notification_rx` 收割迟到的 `assistant_persisted`（与 tool events），文本到达或期满返回；抽取独立函数保证 UT 可测（channel 注入）。同时补齐 `recv_timeout` 与 `Disconnected` 两条路径遗漏的 `parse_tool_event` 收集（工具可见性口径一致）。`swarm_bridge.rs` 空产物失败消息细化为「completed 已到但 assistant_persisted 宽限窗口内未到达（或模型空回复）」，与上游失败/未派发语义可区分。

**不在范围内**（留后续提案）：m3 pipeline 契约对齐 m2 产物文件型（ManagedFile 回流）——pipeline 语义为链式文本传递（`{pipeline_input}` 注入），文件型产物适配 m2 并行独立产物场景；若宽限窗口修复后仍偶发再立项。

## 验收口径

- 渲染模式单跑 `st_gui_journey_real_llm` 不再 SIGSEGV（WSLg 实测）
- ST-S18-16 / st_m3_01 等真实 LLM pipeline 用例回归通过
- `openlogos verify` Gate PASS（覆盖 100%，含新 UT-S18-19/20）
- CPU 红线：CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1
