# 变更提案：add-diagram-suite

> module: core | created: 2026-09-28

## 变更原因
基线文档（add-baseline-docs / complete-other-specs 已归档）以文字描述架构、流程与时序，
缺少可视化规格。架构图、系统处理流程图、时序图能显著降低变更影响分析与新人上手成本，
也是 OpenLogos Phase 3-1（场景建模）建议的产出物。

## 变更类型
设计级（纯文档/图集补充，无代码实现）

## 变更范围
- 新增 `logos/resources/prd/3-technical-plan/1-architecture/diagrams/`：
  - `00-diagram-index.md` 图集总览（总-分导航）
  - `01-system-context.md` 总图：系统上下文
  - `02-kernel-components.md` 分图：内核六组件架构
  - `03-adapter-architecture.md` 分图：端口与适配器
  - `04-backtest-loop.md` 流程：回测主循环（官方三阶段）
  - `05-live-message-flow.md` 流程：实盘消息处理
  - `06-crash-recovery.md` 流程：崩溃恢复
  - `07-seq-order-lifecycle.md` 时序：订单全生命周期
  - `08-seq-backtest-tick.md` 时序：回测逐数据点处理
  - `09-seq-live-reconciliation.md` 时序：实盘对账
  - `10-seq-crash-recovery.md` 时序：崩溃恢复
- `logos/resources/index.md` 与 resource_index 增补条目

## 部署影响
- 是否需要部署：否（纯文档）
- 影响环境：无 / 数据迁移：否 / 回滚：否 / smoke：否

## 变更概述
以 Mermaid 为载体产出"总-分"结构图集：1 张系统上下文总图 +
3 张架构分图 + 3 张处理流程图 + 3 张时序图，共 10 张。
内容以仓库官方文档为真相源（docs/concepts/architecture.md、
docs/concepts/backtesting/execution-flow.md 的官方时序、live 对账概念、
订单类型全集），不虚构组件名。
