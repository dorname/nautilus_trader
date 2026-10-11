# 变更提案：内嵌 octos 运行时并对齐 octoscode 接线与闭环能力

## 变更原因

2026-10-11 接线演练已证实 octos 通路可用：OUP 集成测试 5/5、真实 LLM 单路 turn 完成（glm-5.3）、S17 三路并行真实终态（21.5s）、闭环探针证明默认 Local 会话（Workspace-Write 档）下 agent 工具调用**零审批阻塞**、产物落盘于 octos 托管工作区（`<instance-data-dir>/profiles/<profile>/data/users/<user>/workspace/`）。对照 Octos 官方客户端 octoscode（`/home/kyle/octoscode/`）发现五个缺口，且桌面分发依赖用户 PATH 中的 octos 二进制（本机实测：GUI 进程 PATH 缺 `~/.cargo/bin` 即断线）：

1. **LLM 配置**：profile 缺失静默（`desktop()` 复制用 `let _` 吞错，issue #24 只覆盖 swarm 侧），无 profile 档位发现与引导。
2. **Server 保障**：无 octos 二进制解析与版本 pin（octoscode `backend_ensure`：PATH + `~/.octos/bin` 双通道 + 精确 `REQUIRED_OCTOS_RELEASE`，协议严格匹配）。
3. **接线协议**：手写 JSON-RPC 子集，无能力协商、无工具执行通知（`tool/started`/`tool/completed`）、未复用 `octos-core` crate（octoscode 以 git rev 精确 pin 引入，含完整 UI 协议类型）。
4. **闭环权限**：未利用默认 Workspace-Write 免审批完成工具型任务；托管工作区产物未回流；权限档不可选（`--solo`/`--danger-full-access` 语义未接入，后者按 octoscode 惯例必须人类亲手决策、不入默认）。
5. **octos 未内嵌**：spawn 裸依赖 PATH，与 core-03「运行时随包、不依赖用户 PATH」的既有交付口径（Python 桥接先例）不一致。

附随发现：`gui_walkthrough` 集成测试在功能断言全部通过、三张截图落盘后，于**进程退出阶段**段错误（gdb 定位：glibc `nptl_deallocate_tsd` 调用已卸载库的 TLS 析构函数，wgpu 软渲染库与后台线程退出竞态），导致 exit 101 阻断 `openlogos verify`。需作为测试基建修复纳入本提案。

## 变更类型

设计级（架构 + 场景 + 测试 + 代码；不涉及 API/DB）

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：S17（多 Agent 分析产物回流）、S18（AI 开发工具型生成）
- 影响的部署方案：`prd/3-technical-plan/3-deployment/core-03-desktop-delivery.md`（octos 运行时随包内嵌与版本 pin 章节，对齐 Python 桥接先例）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（桌面无独立编排）
- 影响的测试用例：`core-S17-test-cases.md`、`core-S18-test-cases.md`（新增 server 保障 / 能力协商 / 工具通知 / 产物回流用例）
- 影响的 smoke 测试：无（保持本地启动形态，不新增部署冒烟）

## 部署影响

- 是否需要部署：否
- 部署原因：桌面应用本地运行，无环境部署动作；octos 内嵌进交付形态设计（打包代码属构建产物，非部署执行），与 core-03「本轮只设计，不执行部署」口径一致
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

参考 octoscode 的客户端成熟实践，把 research-desktop 与 octos 的关系从「PATH 依赖 + 手写协议子集」升级为「内嵌运行时 + 类型化协议 + 免审批闭环」：

其一，**Server 保障与内嵌**：新增 `backend_ensure` 模块按「内嵌资源 → `~/.octos/bin` → PATH」顺序解析 octos 二进制并校验 `REQUIRED_OCTOS_RELEASE` 版本 pin（协议严格匹配）；交付形态把 octos 二进制随包分发（对齐 Python 桥接先例），`OupConfig.octos_bin` 不再裸依赖 PATH。

其二，**接线协议**：引入 `octos-core`（git rev 精确 pin，与 pin 的 octos release 对应）替换手写 JSON-RPC 类型；客户端启动时能力协商，处理 `tool/started`/`tool/completed` 通知使 agent 工具执行对 GUI 可见。

其三，**LLM 配置**：`desktop()` 的 profile 复制失败显式报错并给出引导文案；新增 profile 档位发现（扫描 `data_dir/profiles/*.json`）供会话选择模型档。

其四，**闭环权限与产物回流**：S17 三路契约 prompt 升级为产物文件型（分析写入托管工作区文件），产物从 `<instance>/profiles/.../workspace/` 回流挂载到需求版本；权限档以 session/open 参数选择（默认 Workspace-Write），`--danger-full-access` 仅作为人类显式决策的可选项暴露、永不默认。

其五，**测试基建**：修复 `gui_walkthrough` 退出段错误（收尾显式 join 后台线程/受控退出，规避 glibc TLS 析构竞态）；为上述能力补 UT/ST 用例与 OpenLogos reporter。
