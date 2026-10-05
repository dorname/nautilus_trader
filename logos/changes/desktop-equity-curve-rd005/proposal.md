# 变更提案：desktop-equity-curve-rd005

> module: core | created: 2026-10-05

## 变更原因

仓库 issue **RD-005（P2）**：回测实验页净值曲线缺失。原型 `equityChart()` 展示最近两次实验净值折线（面积渐变 + 网格 + 悬停），而桌面 `render_experiments` 仅展示版本数/实验数/最近收益字符串与运行参数。

提案前调查结论（修正 issue 的预设）：**域层无需扩展**——

- `worker_api::RunOutcomeDoc.equity_curve: Vec<EquityDoc>` 早已存在（每日净值：日期/现金/持仓市值/应收/净值）；
- 协调器 `query_rows(object_id, RowsTable::Equity, …)` 读取路径已通（`store::get_research_run` 按 `task_id` 索引，即桌面持有的 `Experiment.task_id` 就是 run 句柄）；
- API 契约 `core-research-contracts.yaml` 已含 QueryRows equity 行类型；
- 桌面 crate 已依赖 `egui_plot`。

真正缺口：桌面桥 `DesktopBridge` 未暴露净值读取、`render_experiments` 未绘制曲线、缺「仅并列查看，不作代码效果归因」横幅。本提案只做**接线与页面**，不动域层协议。

## 变更类型

代码级修复（接口已存在，仅桌面接线 + 页面 + 测试；测试用例文档同步一条 delta）。

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无（core-05「产物与联动」已规定「回测实验：账户指标、最近两次曲线」，本提案补齐实现缺口）
- 影响的业务场景：S19（合成实验比较语义）；场景文档不变
- 影响的部署方案：无
- 影响的 API：无（复用既有 QueryRows equity 契约）
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无
- 影响的测试用例文档：`logos/resources/test/core-S15-test-cases.md`（新增 UT-S15-10 纯函数用例 + ST-S15-05 桥读取用例）
- 影响的代码：
  - `crates/research-desktop/src/bridge.rs`：`EquityPoint`、`equity_curve(run_id)`（分页拉全量）、纯函数 `equity_points()`
  - `crates/research-desktop/src/workspace.rs`：`latest_task_experiments(n)`（最近两次有任务实验选取）、`comparison_banner(a, b)`（三态归因文案，对齐原型 comparisonStatus）
  - `crates/research-desktop/src/app.rs`：`render_experiments` 新增「净值比较」面板（egui_plot 双线 + 最新面积填充 + 网格 + 悬停 + 徽章 + hint），缓存键控懒加载，无数据/运行中/读取失败显式占位
  - `crates/research-desktop/tests/`：UT-S15-10 / ST-S15-05 用例实现
- 同步修改（非 `logos/resources/` 主文档，直接编辑）：
  - `docs/research-desktop/issues/RD-005-*.md`（verify 归档后标记 fixed）

## 部署影响

- 是否需要部署：否
- 部署原因：本地桌面应用代码变更，无服务部署单元
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true             # 回测实验页新增「净值比较」面板（设计以权威原型 equityChart 为准）
design_system_mode: generated
design_system_fallback_reason: ""
pages:
  - id: experiments
    prototype: core-05-ai-workspace-prototype.html
    description: 回测实验页「净值比较」面板——最近两次实验净值折线（最新 #4ade80 + 面积填充、前次灰）、网格与轴标签、悬停数值、E{id}/v{version} 徽章与「仅并列查看」归因横幅
```

## 变更概述

1. **桥接线**：`DesktopBridge::equity_curve(run_id)` 复用协调器 `query_rows(RowsTable::Equity)` 分页拉取全量净值文档，纯函数 `equity_points()` 把 `EquityDoc` 十进制字符串净值解析为绘制点列（解析失败显式报错，不静默丢点）。
2. **页面**：`render_experiments` 在「实验记录」后新增「净值比较」面板——取最近两次有任务的实验，缓存键（task_id 对）变化时懒加载；egui_plot 绘制两条折线（最新绿 `ACCENT_TEXT` 2.5px + 半透明面积填充近似原型渐变、前次灰 #4b5563），网格与日期轴标签，悬停显示「E{id} · 日期 · 净值」；面板顶部横幅复用冻结输入归因语义（`stamp`/`version_id` 三态：输入不同仅并列查看 / 输入一致可归因代码 / 同版本重复实验）；底部 hint「显示最近两次实验。每个点由现金＋持仓市值计算。」与原型一致。
3. **诚实占位**：无任务实验 →「还没有可绘制的实验…」；运行未完成（RunNotReady）→「运行尚未完成，净值曲线待任务终态后可用」；序列为空/读取失败 → 显式错误行。绝不空白冒充已实现（RD-005 期望 3）。
4. **测试**：UT-S15-10 覆盖 `equity_points` 解析（正常/非法/空）、最近两次选取、三态横幅文案；ST-S15-05 复用 ST-S15-04 合成数据链路断言两次运行后 `equity_curve` 非空、日期升序、净值为正。

**明确不做**（留后续）：实验 `total_return` 终态回填（app.rs:3089「当前诚实留空」注释）需协调器新增运行指标读取公开方法，属域层接口扩展，不在本提案；浅色主题、演示筛选 UI 映射同理另立提案。
