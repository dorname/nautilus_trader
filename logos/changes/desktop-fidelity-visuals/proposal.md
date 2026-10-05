# 变更提案：desktop-fidelity-visuals

> module: core | created: 2026-10-05

## 变更原因

高保真核验（对照权威原型 `core-05-ai-workspace-prototype.html` 逐规格审计）发现：
设计 token 级保真已达标（15 色值/字号/断点/布局常量全部一致并有 UT 锁定），
但存在**视觉元素级缺口**——原型中具辨识度的装饰图形未在 egui 端绘制：

1. 左侧栏 11 个路由的 16px stroke 图标（home/file/flow/code/debug/chart/check/plan/data/pool）
2. 项目概览 hero 区 110px 绿双环装饰（hero-orbit，含线性渐变底与窄窗降透明度）
3. empty 态面板 30px flow 大图标
4. 回测实验页净值曲线图（原型 equityChart：最近两实验 polyline + 渐变面积 + 网格）——
   **核验后缩减**：桌面端实验执行委托研究协调器，仅回传汇总 `total_return` 字符串，
   `domain::protocol::Comparison` 亦无逐日净值序列字段；绘制曲线需先扩展域层
   API（属接口级变更）。本提案不含该项，留待后续「净值序列域层扩展」提案处理。
5. hero 线性渐变底（110deg #0f1512 → #0a0b0a，现为纯色均值）

## 变更类型
代码级

## 变更范围
- 影响的需求文档：无（权威原型即设计规格，本提案是实现对齐，不改变设计）
- 影响的功能规格：无
- 影响的业务场景：无（S15 统一工作台实现的视觉补全）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（补测并入既有 UT-S15-08，不新增用例 ID）

## 部署影响
- 是否需要部署：否
- 部署原因：本地原生桌面应用，无部署流水线；由本地构建与 verify 覆盖
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true             # 补全原型既有的装饰图形与图表（设计以权威原型为准）
design_system_mode: generated
design_system_fallback_reason: ""
pages:
  - id: S15
    prototype: core-05-ai-workspace-prototype.html
    description: 导航 SVG 图标 / hero 双环与渐变 / empty 大图标 / 净值曲线——补齐 egui 端缺失的原型视觉元素
```

## 变更概述

以 painter 路径绘制补齐 egui 端缺失的原型视觉元素，全部走纯函数
（输入枚举→输出路径点/颜色），纳入 UT-S15-08 同款纯函数断言体系：

1. **icons 模块**（新 `src/icons.rs`）：把原型 10 组 SVG path
   （viewBox 24×24、stroke-width 1.6、stroke=currentColor）转写为
   egui 点列表常量 + 按路由分发；nav_item 文本左移 24px 为图标腾位，
   激活态图标取 ACCENT_TEXT（原型 `.active svg{color:--accent-text}`）。
2. **hero 双环**：painter 圆 + 双旋转椭圆（40°/−40°）+ 中心点，颜色按
   原型 #22C55E 半透明度（opacity .5，窄窗 .3）；hero 底由纯色改为
   painter 渐变矩形（110° 方向双端色），对齐原型。
3. **empty 大图标**：empty_panel 顶部加 30px flow 图标（原型 `.empty .icon`
   color:#3d5c4a）。
4. **净值曲线（缩减项）**：不实现。原因见「变更原因」第 4 条——桌面端无逐日
   净值数据源，需域层 API 先行扩展（接口级变更，另立提案）。
5. **测试**：UT-S15-08 补图标路径点非空/路由→图标全覆叠/双环与曲线
   颜色常量与原型色值一致等纯函数断言；6 套件 22 用例回归全绿。
