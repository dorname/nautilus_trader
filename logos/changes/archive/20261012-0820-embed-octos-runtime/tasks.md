# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md` — 修订「接入方式」：内嵌运行时解析顺序（内嵌资源 → ~/.octos/bin → PATH）、REQUIRED_OCTOS_RELEASE 版本 pin、octos-core 类型化协议与能力协商、tool/started|completed 通知通路
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-06-multi-agent-architecture.md` — 新增「闭环权限与托管工作区」章节：默认 Workspace-Write 免审批语义、托管工作区路径契约（<instance>/profiles/<profile>/data/users/<user>/workspace/）、产物回流规则、--danger-full-access 人类决策门
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/02-technology-stack.md` — 新增 octos-core（git rev 精确 pin）与 octos 内嵌二进制的依赖条目
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/3-deployment/core-03-desktop-delivery.md` — 「构建与运行依赖」补充：octos 运行时随包分发（对齐 Python 桥接先例）、版本/许可证清单条目、版本 pin 与协议匹配检查
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/2-scenario-implementation/core-S17-ai-project.md` — 时序图与说明更新：三路契约为产物文件型，终态后从托管工作区回流摘要挂载 R_n
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/2-scenario-implementation/core-S18-ai-development.md` — 时序图与说明更新：生成任务经工具通知可见（tool/started|completed），产物回流路径
- [x] 产出 delta 文件到 `deltas/test/core-S17-test-cases.md` — 新增用例：server 保障解析顺序与版本 pin、profile 缺失显式报错、S17 产物文件回流挂载
- [x] 产出 delta 文件到 `deltas/test/core-S18-test-cases.md` — 新增用例：能力协商、工具通知可见性、闭环权限默认档（Workspace-Write 免审批）

## [code] 代码实现

- [x] 新增 `crates/research-desktop/src/backend_ensure.rs`：octos 二进制三级解析（内嵌资源 → ~/.octos/bin → PATH）+ `REQUIRED_OCTOS_RELEASE` 版本校验（逐级降级、GLIBC 坏二进制自动跳过）；打包清单纳入 octos bundle 留交付提案
- [x] `oup.rs` 增强：spawn 前置 backend_ensure 校验；`tool/started|completed` 结构化解析与收割（take_tool_events）；`danger_full_access` 权限档 spawn 参数。范围调整：`octos-core` git 依赖未引入（与离线 verify/cargo-deny 治理冲突，自包含类型已覆盖本批语义，引入列为后续提案）
- [x] `agent_bridge.rs`：`desktop()` profile 复制失败显式报错（profile_warning）+ connect 前置存在性检查；profile 档位发现 `discover_profiles`；托管工作区根路径 `managed_workspace_root`
- [x] `swarm_bridge.rs`：S17 契约 prompt 升级为产物文件型；终态后从托管工作区回流（文件优先、缺失降级 envelope 并按 `output_source` 如实标注；aggregate_valid 口径不变）+ 工具轨迹 tool_trace
- [x] `app.rs`：agent 任务工具执行轨迹入运行记录；Full-Access 两段式决策门（运行记录浮层，默认 Workspace-Write，授权只影响之后新建会话）
- [x] 修复 `gui_walkthrough.rs`/`issue_closure.rs` 退出段错误（glibc TLS 析构竞态，Mesa/llvmpipe 层）：gui_walkthrough 串行模式自动降级无渲染 + exit(0) 计划守卫；issue_closure 渲染移入永不退出的泄漏线程；断言与截图语义不变
- [x] 新用例全部入 reporter：UT-S17-28（backend_ensure 三测）、UT-S17-29（档位发现）、UT-S18-18（默认权限档）、ST-S17-23（产物文件回流端到端，真实 LLM 209.6s 通过）
