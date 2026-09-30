# 规格文档总索引（Master Spec Index）

> 目标：需求文档、设计文档、技术文档以及其他规格 —— 四类齐全，26+2 件
> 生成：2026-09-28 · 治理：OpenLogos（两次提案均 VERIFY_PASS 后归档）

## 一、需求文档（6 篇）· `prd/1-product-requirements/`

| 文件 | 内容 |
|---|---|
| 01-overview.md | 产品定位与价值主张（回测-实盘割裂痛点、引擎+库形态） |
| 02-user-personas.md | 用户画像 P1-P4 + 反画像 |
| 03-user-stories.md | 用户故事 US-001~US-010 含验收条件 |
| 04-functional-requirements.md | 功能性需求 FR-001~FR-018（含优先级） |
| 05-non-functional-requirements.md | 非功能性需求 NFR-001~NFR-012 |
| 06-competitor-landscape.md | 竞品格局与差异化 |

## 二、设计文档（2 篇）· `prd/2-product-design/`

| 文件 | 内容 |
|---|---|
| 1-feature-specs/01-information-architecture.md | 信息架构 + 能力域↔crate↔需求追踪矩阵 |
| 2-page-design/01-no-gui-design-decision.md | 无 GUI 设计决策（ADR） |

## 三、技术文档（6 篇）· `prd/3-technical-plan/`

| 文件 | 内容 |
|---|---|
| 1-architecture/01-architecture-overview.md | 架构总览（六组件内核/DDD/事件溯源/crash-only） |
| 1-architecture/02-technology-stack.md | 技术选型（Rust+PyO3+Python）与权衡 |
| 1-architecture/03-runtime-dataflow.md | 回测/实盘运行时数据流与订单状态机 |
| 2-scenario-implementation/00-scenario-implementation-map.md | 场景→实现组件映射（ST-01~08） |
| 3-deployment/01-deployment-model.md | 部署模型（wheel/cargo/源码三形态） |
| 3-deployment/02-smoke-test-spec.md | 冒烟测试规格 SMK-01~05 |
| 1-architecture/diagrams/00~09（10 张） | 图集：架构×3 + 流程×3 + 时序×3 + 总览（总-分结构） |

## 四、其他规格（14 件）

| 类别 | 文件 | 内容 |
|---|---|---|
| 测试用例 | test/unit/unit-test-cases.md | 单元用例 UT 18 条（含 AC 溯源） |
| 测试用例 | test/scenario/scenario-test-cases.md | 端到端用例 ST 8 条（含 AC 溯源） |
| 接口规格 | api/01-interface-index.yaml | Python/Rust/CLI 公共接口索引（替代 OpenAPI） |
| 实现规格 | implementation/01-implementation-inventory.md | 26 crates + 18 适配器实现清单 |
| 场景编排 | scenario/st-01.json ~ st-08.json | 8 个机器可读编排规格 |
| 数据库规格 | database/00-no-database-decision.sql | 无自有数据库决策记录（ADR） |
| 验收产物 | verify/acceptance-report.md · test-results.jsonl | Gate PASS 报告与 26/26 结果 |

## 验收状态

- `openlogos verify`：Gate 3.6 **PASS**（覆盖 26/26，通过率 100%，AC 追溯 5/5）
- 提案归档：`changes/archive/20260928-0833-add-baseline-docs/`、`changes/archive/20260928-0931-complete-other-specs/`
- 全部 8 类声明目录非空；每件文档在 `logos-project.yaml` resource_index 中有索引条目

## A股低频研究桌面设计

本组设计属于 astock-research-desktop，规格已合并，应用尚未实现或验收。

- `api/core-research-contracts.yaml`
- `database/core-01-research-storage.sql`
- `prd/1-product-requirements/01-overview.md`
- `prd/1-product-requirements/core-08-research-requirements.md`
- `prd/2-product-design/1-feature-specs/core-02-research-workflow.md`
- `prd/2-product-design/2-page-design/01-no-gui-design-decision.md`
- `prd/2-product-design/2-page-design/core-02-research-pages.md`
- `prd/3-technical-plan/1-architecture/core-05-research-architecture.md`
- `prd/3-technical-plan/2-scenario-implementation/core-01-research-scenarios.md`
- `prd/3-technical-plan/2-scenario-implementation/core-S11-data-snapshot.md`
- `prd/3-technical-plan/2-scenario-implementation/core-S12-universe.md`
- `prd/3-technical-plan/2-scenario-implementation/core-S13-research-run.md`
- `prd/3-technical-plan/2-scenario-implementation/core-S14-compare.md`
- `prd/3-technical-plan/2-scenario-implementation/core-S15-trade-plan.md`
- `prd/3-technical-plan/3-deployment/core-03-desktop-delivery.md`
- `scenario/core-research-orchestration.json`
- `test/core-09-research-test-cases.md`
- `test/core-S11-test-cases.md`
- `test/core-S12-test-cases.md`
- `test/core-S13-test-cases.md`
- `test/core-S14-test-cases.md`
- `test/core-S15-test-cases.md`
- `test/smoke/core-desktop-smoke-test-cases.md`

本组规格与资源索引已同步；保留next_id=20及S11～S19编号。原验收状态仅适用于原基线，不可沿用为桌面工具验收。


## 高保真桌面原型入口

- [打开交互原型](prd/2-product-design/2-page-design/core-03-research-prototype.html)：浏览器直接打开，离线运行，刷新重置演示状态。
- [原型设计说明](prd/2-product-design/2-page-design/core-03-research-design.md)：视觉、交互范围和原型检查用例。
- 原型已在Chromium检查UI-P01～11，全部通过；这是HTML交互检查，不代表Rust应用或业务回测已实现。

- [策略开发设计](prd/2-product-design/2-page-design/core-04-strategy-development-design.md)：新增第六页，支持草稿、源码与参数编辑、版本冻结和研究绑定；可在HTML地址后加 `#develop` 直接进入。

## AI 策略研究工作台

- [打开新版工作台](prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html)：需求→多Agent协作→处理流程→开发→事件调试→回测比较。
- [工作台设计](prd/2-product-design/2-page-design/core-05-ai-workspace-design.md)：角色分工、版本与可见事件、原生GUI实现方向和演示边界。
- [补充需求](prd/1-product-requirements/core-09-ai-workspace-requirements.md)：FR-R13～17与S17～S19。
- 8项原型编排检查通过；Agent为预设演示，实验由固定合成样本计算。没有接入真实模型或生产回测引擎。

## 统一策略研究原型入口

唯一入口：[研序统一工作区](prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html)。旧 core-03 入口兼容导向同一应用。完整旅程：对话需求 → 设计图 → 开发 → 调试和实验 → 验证报告 → 演示计划核对及导出。

设计见 core-05-ai-workspace-design.md；检查见 S17～S20，证据保留在变更目录 prototype-review，不代表生产验收。
