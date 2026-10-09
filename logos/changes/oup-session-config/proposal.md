# 变更提案：oup-session-config

> module: core | created: 2026-10-09

## 变更原因
关联 issue：**#27**（OUP session_id 未与桌面 project_id 绑定，项目隔离失效）、
**#24**（SwarmBridge 硬编码 /home/kyle/.octos profile 路径）、**#14**
（OUP 子进程未注入 LLM API key / cwd，桌面环境易静默无模型）、**#20**
（stdio 下 session/messages_page 503，能力子集需文档化）。

四者同属 **OUP 会话配置**主题：会话归属、凭据与进程环境是同一层
「子进程如何被正确拉起」的问题。

现状核实（turn-lifecycle 之后）：
- `AgentSessionConfig::desktop` 已生成 `research-{project_id}` session 并解析
  profile（env/HOME 回退），但 **GUI 切换/新建项目时不重置 worker**——旧
  worker 仍持旧项目 session，隔离失效（#27 剩余缺口）；
- `swarm_bridge.rs` 派发路径仍硬编码 `/home/kyle/.octos/profiles/octos.json`
  且缺失时静默跳过（#24）；
- `OupClient::spawn` 不传 cwd、`stderr(Stdio::null())`、不显式注入 profile
  `config.env_vars`（#14）；
- stdio 能力子集（messages_page 不可用）未见诸文档（#20）。

## 变更类型
代码级（接口小幅扩展：OupConfig 增 extra_env；行为修正：项目切换重置
worker、profile 缺失早失败、子进程 cwd/stderr/env 显式化）

## 变更范围
- 影响的需求文档：无（core-06 §5.6 项目隔离口径的实现闭环）
- 影响的功能规格：core-S17 测试用例（新增 UT-S17-23 / UT-S17-24）
- 影响的业务场景：S17/S19 多 Agent 派发与对话任务的会话归属与进程环境
- 影响的 API：OupConfig + extra_env；AgentWorker/SwarmWorker + session_id()；
  ResearchApp 项目切换/新建时重置 worker（后台销毁不阻塞渲染）
- 影响的 DB 表：无
- 影响的编排测试：m2_swarm_integration.rs（新增 UT 用例）

## 部署影响
- 是否需要部署：否
- 部署原因：库与桌面行为修正，无独立部署物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false            # 本次是否触及界面（GUI 项目才有意义）
design_system_mode: generated   # generated | fallback（fallback 时须填 design_system_fallback_reason）
design_system_fallback_reason: ""
pages: []                   # 每项 {id, prototype: core-NN-<slug>.html, description}
```

## 变更概述
1. **#27 项目隔离闭环**：AgentWorker/SwarmWorker 记录构造时 session_id；
   GUI 切换/新建项目时后台销毁旧 worker（不阻塞渲染）、放弃在途任务引用
   并记录日志，下次任务惰性重建（绑定新项目 session）。
2. **#24 去硬编码**：swarm 派发的子 agent profile 复制改为读 base 配置
   `data_dir/profiles/octos.json`（desktop_with_profile 已解析落盘）；
   profile 缺失时明确返回错误（不静默跳过）。
3. **#14 子进程环境**：OupClient::spawn 以 `workspace_root` 为 cwd；
   stderr 落盘 `{data_dir}/logs/octos-stderr.log`（排障可见）；
   AgentBridge::connect 读取 profile `config.env_vars` 显式注入子进程环境
   （不写仓库、优先用户已 export 的同名变量语义保持覆盖一致）。
4. **#20 文档化**：oup.rs 模块注释写明 stdio 能力子集（messages_page 为
   REST-only，桌面读产物走 notification / turn/state/get）。
5. 测试：UT-S17-23（项目隔离矩阵 + worker 重建，无 LLM）、UT-S17-24
   （子进程 cwd/stderr/env 注入，spawn 级不调 LLM）。
