# 设计决策：无 GUI（No GUI Design Decision）

> 状态：基线（ADR 形式，记录既有事实） · 影响：Phase 2 页面设计阶段整体跳过

## 决策

NautilusTrader **不提供图形界面**。2-page-design 目录下不产出页面设计文档与 HTML 原型。

## 背景

产品是交易**引擎/框架（库）**，用户是开发者与研究员（见 personas P1–P4），其工作流是
写代码 → 跑回测 → 部署节点，全部通过 API/CLI/日志完成。仓库内不存在任何前端代码。

## 理由

1. **用户是程序员**：目标用户全部以代码为主要交互手段，GUI 非其工作流的一部分
2. **性能与可靠优先**：引擎追求低延迟与 crash-only 恢复，GUI 是额外故障面与维护负担
3. **可组合性**：库形态允许用户自建监控面板（经日志/事件导出/消息总线外部 backing 对接 Grafana 等）
4. **生态分工**：可视化需求由外部工具承接（见 docs/concepts/visualization.md 的辅助输出），引擎不越界

## 影响

- OpenLogos Phase 2 的"页面设计"阶段对本项目标记为**不适用**（本 ADR 即该阶段的正式产出）
- 任何未来 GUI 提案属于需求级变更，需走 `openlogos change` 全链路评审
- 用户交互规格由以下文档承载：
  - Python API：docs/api_reference + python 包 docstring
  - Rust API：docs/api_reference（Rust）+ crates doc
  - CLI：crates/cli
