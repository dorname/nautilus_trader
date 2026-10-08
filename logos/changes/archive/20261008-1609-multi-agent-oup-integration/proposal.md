# 变更提案：多 Agent OUP 集成（M1-M4）

> 模块：core；分支：a-stock；状态：实现已完成，待合规化验收
> 关联场景：S17（需求与多Agent协作）、S18（流程与开发）、S19（调试与实验）

## 变更原因

OpenLogos 原型（core-05）中的多 Agent 协作目前只有离线 HTML 演示，无生产实现。用户要求基于 octos 内核（octos-agent/octos-swarm）实现可落地的多 Agent 架构，接入方式为 OUP（Octos UI Protocol）over stdio，参考 octoscode 的 client/server 划分。

## 变更类型

实现级变更：在 research-desktop 中新增 OUP 客户端、Agent 桥接、Swarm 编排模块，并配套集成测试。本次交付完整实现代码与验收测试，不执行部署。

## 变更概述

以本地 octos serve 为 Agent 运行时底座，research-desktop 通过 stdio JSON-RPC 与之通信。实现 M1-M4 四个批次：

- **M1**：OUP 客户端（session/open、turn/start、task/list、agent/list）
- **M2**：Swarm Parallel 并发派发（3 上限，S17 三契约：数据可行性/反例检索/约束核对）
- **M3**：Pipeline 串行拓扑（S18 设计→代码检查，{pipeline_input} 占位符注入）
- **M4**：S19 实验契约（版本冻结引用 + 证据边界声明 + 协调器集成约束）

CPU 保护：并发上限 MAX_PARALLEL_DISPATCH=3、每个子 agent 独立数据目录避免锁冲突、静默零帧（500ms 轮询）。

## 变更范围

### 新增文件

| 文件 | 行数 | 说明 |
|---|---|---|
| `crates/research-desktop/src/oup.rs` | 335 | OUP 客户端：JSON-RPC 编解码、子进程管理 |
| `crates/research-desktop/src/agent_bridge.rs` | 265 | Agent 会话：connect/disconnect/submit_prompt |
| `crates/research-desktop/src/swarm_bridge.rs` | 507 | Swarm 编排：Parallel/Pipeline、S17/S18/S19 契约 |
| `crates/research-desktop/tests/m1_oup_integration.rs` | 137 | 5 ST：OUP 通信验证 |
| `crates/research-desktop/tests/m2_swarm_integration.rs` | 185 | 5 ST：并发派发验证 |
| `crates/research-desktop/tests/m3_pipeline_integration.rs` | 156 | 4 ST：Pipeline 拓扑验证 |
| `crates/research-desktop/tests/m4_experiment_integration.rs` | 101 | 4 ST：S19 实验契约验证 |
| `logos/resources/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md` | 134 | 架构设计文档 |

### 修改文件

- `crates/research-desktop/Cargo.toml`：新增 serde、uuid 依赖
- `crates/research-desktop/src/lib.rs`：注册 oup、agent_bridge、swarm_bridge 模块

## 验收证据

### 测试输出

```
running 5 tests
test st_m1_01_oup_spawn_session_open ... ok
test st_m1_02_oup_task_list ... ok
test st_m1_03_oup_agent_list ... ok
test st_m1_04_oup_invalid_method ... ok
test st_m1_05_oup_concurrent_spawn ... ok
test result: ok. 5 passed; 0 failed

running 5 tests
test st_m2_01_swarm_dispatch_parallel ... ok
test st_m2_02_s17_contracts_dispatch ... ok
test st_m2_03_dispatch_exceeds_limit ... ok
test st_m2_04_cancel_all ... ok
test st_m2_05_poll_all ... ok
test result: ok. 5 passed; 0 failed

running 4 tests
test st_m3_01_pipeline_dispatch ... ok
test st_m3_02_s18_contracts ... ok
test st_m3_03_pipeline_exceeds_limit ... ok
test st_m3_04_pipeline_no_upstream ... ok
test result: ok. 4 passed; 0 failed

running 4 tests
test st_m4_01_experiment_dispatch ... ok
test st_m4_02_experiment_version_freeze ... ok
test st_m4_03_empty_version_rejected ... ok
test st_m4_04_experiment_evidence_boundary ... ok
test result: ok. 4 passed; 0 failed
```

### 回归测试

- 33 UT + 22 S15-ST + 5 S17-S20-ST 全部通过，零回归

## 生产约束（继承 core-05/core-06）

1. 数值结果必须来自协调器计算任务，LLM 不得生成回测数值
2. 未确认需求不能生成设计
3. 任意源码不可冒称执行（code-reviewer 只接受预置示例完整匹配）
4. 正式验证恒需真实证据（真实数据/样本外/稳健性缺失时显示"证据不足"）
5. 幂等与恢复：dispatch_id + 契约指纹，GUI 重启后从 redb 重建状态
6. 项目隔离：session_id 绑定项目 ID
7. 零新增 unsafe

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| octos serve 版本兼容性 | 锁定 octos 2.0.3-rc.9，profile 配置从主数据目录复制 |
| 并发 spawn 数据目录锁冲突 | 每个子 agent 独立 `data_dir`/`instance_dir` |
| LLM 调用延迟导致 Pipeline 超时 | 串行派发不等待完成，异步通知由事件循环处理 |
| 测试环境无 LLM provider | 从主 octos 数据目录复制 profile（含 moonshot-coding 配置） |
