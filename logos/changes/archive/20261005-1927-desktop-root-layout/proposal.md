# 变更提案：desktop-root-layout

> module: core | created: 2026-10-05

## 变更原因

真实 GUI 目检（用户截图）发现根布局塌缩：左侧栏 212px 玻璃卡、主区大卡
（顶栏/画布/对话栏/footer）全部消失，所有内容挤成顶部一行，文字竖排挤压、
画布与对话栏不可见。

根因：egui 0.36 布局模型误用。根 `allocate_ui` + `ui.horizontal` 内，
`render_sidebar`/`render_main` 的 `Frame::NONE.show` 只包裹内容自然尺寸；
侧栏内 `ui.set_min_size(vec2(w, ui.available_height()))` 在 horizontal 布局中
拿到的 `available_height()` 是**行高而非全窗高**，于是各子 Ui 宽度退化为
内容固有宽，整条 horizontal 只有一行高，竖排文字是极窄宽度逐字换行的产物。
纯函数测试覆盖不到真实 egui 布局路径，故 verify 全绿但视觉塌缩。

## 变更类型
代码级

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无（权威原型三区布局规格不变，本次是实现缺陷修复）
- 影响的业务场景：无（S15 统一工作台布局骨架实现修正）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（补测并入既有 UT-S15-08，不新增用例 ID）

## 部署影响
- 是否需要部署：否
- 部署原因：本地原生桌面应用；由本地构建、截图目检与 verify 覆盖
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true             # 修复真实渲染下的三区布局塌缩，恢复原型骨架
design_system_mode: generated
design_system_fallback_reason: ""
pages:
  - id: S15
    prototype: core-05-ai-workspace-prototype.html
    description: 根三区布局（侧栏212/178 + 主卡65顶栏+画布+360/320对话栏+28 footer）在真实 egui 渲染下正确成形
```

## 变更概述

把根三区从「horizontal + Frame 自适应」改为「显式矩形切分」：

1. 根 Ui 取 `ui.max_rect().shrink(APP_GAP)` 得 outer 矩形；按原型规格切列——
   sidebar 固定宽（212/178）、间隙 12、main 吃剩余宽；列间用
   `ui.allocate_new_ui(Layout::top_down(Align::Min), rect)` 建独立列 Ui。
2. 侧栏列内垂直排布（品牌/项目/两组导航/底部声明），底部声明用
   `Layout::bottom_up` 子 Ui 固定在列底；主卡列内垂直三段——顶栏 65、
   body = 列高 − 65 − 28、footer 28（高度由列矩形算，不再用
   horizontal 内的 available_height()）。
3. render_canvas/render_chat 在 body 行内同样按显式矩形切分
   （canvas = body 宽 − chat 宽）。
4. **测试**：UT-S15-08 补「列切分纯函数」断言（layout 新增
   `columns(outer_w, plan) -> (sidebar_w, main_w)` 与
   `rows(outer_h) -> (topbar, body, footer)`，输入异常宽/高时各段非负、
   和不超过 outer）；6 套件 22 用例回归 + 截图目检对照原型。
