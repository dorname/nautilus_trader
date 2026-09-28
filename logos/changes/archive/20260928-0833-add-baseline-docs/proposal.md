# 变更提案：add-baseline-docs

> module: core | created: 2026-09-27

## 变更原因

项目以 `openlogos adopt` 存量接入方式初始化 OpenLogos，Phase 1-3-0 的文档基线被跳过，
`logos/resources/` 下所有文档目录（prd / api / database / test / scenario / implementation）均为空。
项目缺少统一的"真相源"文档：需求文档、产品设计文档、技术架构文档及其他规格（测试用例规格、
部署冒烟规格、资源索引）全部缺失，导致后续任何变更提案都无法进行影响范围分析（change-writer
依赖 `logos/resources/` 中的现有文档定位受影响范围）。

本提案补齐存量项目的文档基线，不改变任何代码行为。

## 变更类型

需求级（纯规格补全，无代码实现）

## 变更范围

- 影响的需求文档：`logos/resources/prd/1-product-requirements/`（新增全量基线）
  - `01-overview.md` 产品定位与价值
  - `02-user-personas.md` 用户画像
  - `03-user-stories.md` 用户故事（US-001 ~ US-010）
  - `04-functional-requirements.md` 功能性需求（FR-001 ~ FR-018）
  - `05-non-functional-requirements.md` 非功能性需求（NFR-001 ~ NFR-012）
  - `06-competitor-landscape.md` 竞品格局
- 影响的功能规格：`logos/resources/prd/2-product-design/1-feature-specs/`（新增）
  - `01-information-architecture.md` 信息架构与功能规格总览
- 影响的页面设计：`logos/resources/prd/2-product-design/2-page-design/`（新增）
  - `01-no-gui-design-decision.md`（本项目为引擎/库，无 GUI，记录该设计决策）
- 影响的技术方案：`logos/resources/prd/3-technical-plan/1-architecture/`（新增）
  - `01-architecture-overview.md` 架构总览（事件驱动 + DDD + 端口适配器 + crash-only）
  - `02-technology-stack.md` 技术选型（Rust 内核 + Python 控制面 + PyO3）
  - `03-runtime-dataflow.md` 运行时数据流（DataEngine/RiskEngine/ExecutionEngine/Portfolio/MessageBus/Cache）
- 影响的部署方案：`logos/resources/prd/3-technical-plan/3-deployment/`（新增）
  - `01-deployment-model.md` 部署模型（单节点引擎、非服务化部署）
  - `02-smoke-test-spec.md` 冒烟测试规格
- 影响的测试规格：`logos/resources/test/`（新增）
  - `unit/`（模块单元测试规格）与 `scenario/`（端到端场景测试规格）
- 影响的资源索引：`logos/logos-project.yaml` 的 `resource_index`（merge 后重建）
- 影响的 API：无（引擎为库，OpenAPI 不适用；以 Python/Rust 公共接口清单规格替代 →
  `logos/resources/api/` 下以 interface-index 形式记录）
- 影响的 DB 表：无（引擎无自有数据库；持久化形态记录于架构文档）
- 影响的编排测试：无（`logos/resources/scenario/` 目录保留为空，理由记录于提案)

## 部署影响

- 是否需要部署：否
- 部署原因：本提案为纯文档变更，不触碰源码、构建产物与运行时环境
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

本提案为存量项目（nautilus_trader，Rust 1.98.1 / Python 3.12-3.14，版本 v2.0.0rc6）
补齐 OpenLogos 全套基线文档。文档内容以仓库内既有事实为真相源逆向整理：
README.md、docs/concepts/（architecture.md、overview.md、backtesting/、live.md 等）、
Cargo.toml workspace 结构（26 个 crates、18 个 venue 适配器）、ADAPTERS.md、
MIGRATION_V2.md、Makefile、CI 配置。

产出分四层：

1. **需求层（WHY）**：产品定位（生产级多资产多场所事件驱动交易引擎）、用户画像
   （量化研究员、量化开发者、自营交易团队）、10 条用户故事、18 条功能性需求、
   12 条非功能性需求（延迟、确定性回测、事件溯源、崩溃恢复）。
2. **设计层（WHAT）**：信息架构——26 个 crates 与 Python 包镜像的职责矩阵、
   功能规格与需求的双向追踪。
3. **技术层（HOW））**：架构总览（NautilusKernel 六组件、消息总线、缓存、事件流）、
   技术选型及理由、运行时数据流、部署模型与冒烟规格。
4. **其他规格**：单元/场景测试用例规格（对接 `cargo test` 既有测试体系）、
   公共接口索引、资源索引。

不产出代码，不修改任何源文件。merge 后执行 `openlogos index` 重建 resource_index。
