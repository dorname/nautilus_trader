# 变更提案：修复 issue #38（相对路径致 octos 不可用）与 #39（需求草稿 LLM 生成）

## 变更原因

- **issue #38（bug）**：本地源码启动默认相对路径 `research-workspace/octos-agent`，`OupClient::spawn` 以 `current_dir(workspace_root)` 启动子进程后，`--data-dir` 相对路径在子进程内二次解析为嵌套路径，octos 在嵌套空目录初始化 profile store → `session/open` 报 `-32602 profile 'octos' is not configured for this AppUI session`。影响面：多 Agent 分析派发、自由对话（降级固定文案）、设计/代码生成、模型设置浮层全部不可用。已最小复现（相对/绝对对照矩阵）。
- **issue #39（增强）**：用户期望对话中描述研究想法时助手隐式调用 agent 生成结构化需求草稿；当前 `DraftRequirement` 意图仅回固定预设文案并把原文塞入草稿框，不经 LLM。

## 变更类型

需求级（小范围：一个可靠性修复 + 一个对话动作在线化；无 API/DB/部署变更）

## 变更范围

- 影响的需求文档：`prd/1-product-requirements/core-09-ai-workspace-requirements.md`（新增 FR-R23；边界段补路径口径）
- 影响的功能规格：`prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`（需求草稿 LLM 生成口径）
- 影响的业务场景：S17（agent 会话路径口径回归用例）、S22（在线对话链路修复受益）、新增 S23（需求草稿 LLM 生成）
- 影响的部署方案：无
- 影响的 API：无（复用既有 OUP turn 通道）
- 影响的 DB 表：无
- 影响的编排测试：无（桌面项目无 API 编排）
- 影响的 smoke 测试：无

## 部署影响

- 是否需要部署：否
- 部署原因：纯桌面本地功能修复，无服务部署
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

1. **#38 修复**：`AgentSessionConfig::desktop_with_profile` 对 `base_dir` 做 canonicalize（创建目录后解析为绝对路径），`OupClient::spawn` 传参前对 `data_dir`/`instance_dir`/`workspace_root` 防御性 canonicalize——保证父子进程路径视图一致；新增相对路径回归用例堵测试盲区。
2. **#39 增强**：`DraftRequirement` 意图改道真实 LLM——以用户消息为输入，经 AgentWorker 生成结构化需求草稿（研究目标/范围/参数/验收标准），落入需求草稿框等用户编辑确认；解析失败或 octos 不可用时诚实降级为当前预设行为（原文入草稿框）并如实标注。
