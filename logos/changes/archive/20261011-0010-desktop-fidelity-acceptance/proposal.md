# 变更提案：desktop-fidelity-acceptance

> module: core | created: 2026-10-10

## 变更原因
Issue #3（RD-003）：egui 实现与 core-05 原型存在视觉/交互缺口。经逐项核对，
代码侧缺口已在先前变更中修复（品牌渐变 mark、导航辉光、≤900 图标档、内嵌 CJK 字体），
但 issue 要求的核心交付物——「必须像素/交互对齐」与「平台近似可接受」两张验收口径表——
尚未落入验收规格，导致视觉验收缺少书面基准。

## 变更类型
设计级（验收口径文档化，无代码变更）

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：`logos/resources/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`（新增「视觉保真验收口径」一节）
- 影响的业务场景：S15（桌面工作台视觉验收）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无

## 部署影响
- 是否需要部署：否
- 部署原因：纯规格文档变更
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true            # 涉及视觉验收口径（不改代码，仅文档化既有实现与近似）
design_system_mode: generated
design_system_fallback_reason: ""
pages: []
```

## 变更概述
在 core-05 设计文档新增「视觉保真验收口径」一节，含两张表：
（1）必须像素/交互对齐项——品牌 mark 145° 渐变+辉光、导航激活指示条+辉光、
三档布局断点（212/178/65）、文字层级与等宽数字、键盘焦点可见；
（2）平台近似可接受项——backdrop-filter 毛玻璃以半透明面板+高光描边近似、
box-shadow 辉光以多层透明度叠加近似、双径向柔光以角点线性插值近似、
prefers-reduced-motion（当前无动画，未来引入时须尊重）。
同时显式记录两项关闭声明：浅色主题口径作废（RD-002 归档结论）、
中文字体已随包内嵌（NotoSansSC-Regular.otf，系统探测优先、内嵌兜底）。
