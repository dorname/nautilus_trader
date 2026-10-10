# 合并指令

## 变更提案
- 提案名称：desktop-fidelity-acceptance
- 提案目录：logos/changes/desktop-fidelity-acceptance/

## 提案内容

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


## 需要合并的 Delta 文件

### 1. deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md

- Delta 文件：`logos/changes/desktop-fidelity-acceptance/deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`
- 目标目录：`logos/resources/prd/2-product-design/2-page-design/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(desktop-fidelity-acceptance): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive desktop-fidelity-acceptance`。
