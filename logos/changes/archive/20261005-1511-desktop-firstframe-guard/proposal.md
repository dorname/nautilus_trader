# 变更提案：desktop-firstframe-guard

> module: core | created: 2026-10-04

## 变更原因

Bug 反馈：桌面应用（research-desktop）在 WSLg 环境下启动即 panic——
`Negative desired size: [-335.8 158.0]`（egui 0.36.2 `Ui::allocate_ui` 断言）。

根因：winit 在该环境下首帧返回的窗口尺寸（约 260×267）远小于 `with_inner_size`
请求的 1440×900（`with_min_inner_size` 亦未在首帧生效），且 Wayland / X11 后端下
数值一致（复现 2/2）。`render_body` 将 `主区可用宽 − chat_w` 直接喂给
`render_canvas` 的 `allocate_ui`，负值触发断言崩溃。属首帧极端尺寸未防御的
健壮性缺陷，正常桌面环境（尺寸就绪后）不受影响。

## 变更类型
代码级

## 变更范围
- 影响的需求文档：无（对外行为不变，属崩溃防御）
- 影响的功能规格：无
- 影响的业务场景：无（S15 桌面统一工作台实现层的健壮性加固）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（新增断言并入既有 UT-S15-08 布局规格用例，不新增用例 ID）

## 部署影响
- 是否需要部署：否
- 部署原因：桌面应用为本地原生程序，无部署流水线；修复后由本地构建与 verify 覆盖
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false            # 防御性修复，正常环境下界面渲染结果不变
design_system_mode: generated   # generated | fallback（fallback 时须填 design_system_fallback_reason）
design_system_fallback_reason: ""
pages: []                   # 每项 {id, prototype: core-NN-<slug>.html, description}
```

## 变更概述

两层防御，消除首帧（及任意极端窄窗）下的负尺寸 panic：

1. **首帧守卫**（`app.rs` `App::ui` 入口）：当根矩形宽或高低于阈值
   （宽 < 400 或高 < 200）时跳过本帧绘制并 `request_repaint()`——
   等待 winit 尺寸就绪后下一帧正常渲染；正常环境仅首帧白一帧，无感知。
2. **宽度钳制双保险**（`app.rs` `render_body`/`render_canvas`/`render_chat`
   等所有 `allocate_ui` 入参）：对计算的宽高取 `max(0.0)`，任何极端窗口
   尺寸下不再出现负 desired size。
3. **测试**：在 `tests/s15_desktop_l2.rs` 的 UT-S15-08 布局规格用例中补
   窄窗（260×267 同款输入）下布局纯函数输出钳 0、不为负的断言
   （无 GUI 离线可测），并回归 6 套件 22 用例。
