# 变更提案：desktop-issue-docs-rd002-rd004

> module: core | created: 2026-10-05

## 变更原因

修复仓库 issue 中两个提案级文档/产品口径问题（`docs/research-desktop/issues/`）：

- **RD-002（P1）**：`core-02-research-pages.md` 与 `core-05-ai-workspace-design.md` + 原型 HTML 在主题（浅色 vs 纯黑科技深色）、默认窗口（1440×900 vs 可操作基准 1440×1000/1100×900）、侧栏宽（180 vs 212/178/65）、顶栏（56 vs 65）、底栏（32 任务状态 vs 28 footer）、信息架构（五页线框 vs 11 路由统一工作台）六处冲突。`core-03-research-design.md` 已声明「原六页能力迁入 core-05」，但 core-02 正文未降级为历史附录，仍可读作现行规格，造成验收歧义。实际实现 `crates/research-desktop` 已全面对齐 core-05。
- **RD-004（P2）**：数据中心 / 股票池两页，HTML 原型为演示交互（合成快照/模拟更新/搜索+市场筛选），生产桌面为协调器协议驱动 UI（CSV 导入/快照清单、RuleAST JSON 预览）。二者混用且无文档声明，验收时易误判为 UI 不合格。

## 变更类型

设计级（文档口径澄清：不新增/修改任何行为或接口，仅把已实现的权威关系与分叉决策写入规格）。

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的产品设计（页面设计）：
  - `logos/resources/prd/2-product-design/2-page-design/core-02-research-pages.md`（RD-002：降级为历史线框并指向 core-05，冲突数字统一到单一来源，删除「默认浅色」）
  - `logos/resources/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`（RD-004：新增「演示原型与生产桌面映射」章节，声明资源页分叉口径）
- 影响的业务场景：无（S11/S12 协调器契约不变；仅验收口径写明资源页不以 HTML 像素对齐）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无
- 同步修改（非 `logos/resources/` 主文档，直接编辑）：
  - `logos/logos-project.yaml`（core-02/core-05 资源描述同步，避免 AI/验收再读到冲突口径）
  - `docs/research-desktop/issues/RD-002-*.md`、`RD-004-*.md`、`README.md`（标记 fixed）

## 部署影响

- 是否需要部署：否
- 部署原因：纯规格文档与索引描述修订，不涉及任何运行时组件
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false            # 不触及界面实现；仅澄清设计文档口径（桌面 UI 已对齐 core-05）
design_system_mode: generated
design_system_fallback_reason: ""
pages: []
```

## 变更概述

**RD-002**：将 `core-02-research-pages.md` 的「界面结构」章节改写为降级声明——明确现行 UI 权威规格 = core-05（设计文档 + 原型 HTML + `layout.rs`/`theme.rs` 常量），core-02 线框保留为历史信息架构参考；窗口尺寸唯一来源为 `theme::DEFAULT_WINDOW 1440×900 / MIN_WINDOW 1100×720`（core-05 的 1440×1000/1100×900 为原型可操作基准，非冲突）；侧栏 212/178/65、顶栏 65、底栏 28 以 core-05 与实现常量为准；删除「默认浅色，支持深色」——现行仅深色「纯黑科技 v3」，若未来需要浅色主题须单独立变更提案。

**RD-004**：采纳 issue 建议的方案 2「声明分叉」——生产桌面已对接协调器真实接口（数据导入、快照清单、规则预览均为真功能），对齐演示原型意味着回退为演示壳，与生产目标相悖。在 `core-05-ai-workspace-design.md` 新增「演示原型与生产桌面映射」章节：HTML 原型 `#data`/`#pool` 为演示交互（合成数据、模拟更新）；生产桌面数据中心/股票池以协调器协议 UI 为准；验收不对这两页做 HTML 像素级对齐，业务正确性以 S11/S12 协调器契约与场景测试为准。
