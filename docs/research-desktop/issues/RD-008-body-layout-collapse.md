# RD-008 主工作区 body 布局塌缩（画布内容竖排挤压 / 对话栏消失）

- **类型**：bug / UI 布局
- **优先级**：P0
- **状态**：fixed（2026-10-06，提案 desktop-body-layout-rd008——F1 body 显式矩形切列 + top_down 子 Ui；F2 指标卡/侧栏底部/对话栏底带同类点修复；F3 ST-S15-06 无头整页几何回归双窗口档通过）
- **权威原型**：`core-05-ai-workspace-prototype.html`
- **实现**：`crates/research-desktop/src/app.rs`（`render_body` / `render_canvas` / `render_chat`）

## 现象（用户截图，最大化窗口 ~1750×900）

1. 主工作区 Overview 内容整体不可见（hero 卡片 / 指标卡 / 研究路径均未显示）。
2. 右侧「研究助手」对话栏整体消失，窗口右缘残留被裁切的文本碎片。
3. workspace-head（◇ 项目产物行）被垂直居中到 body 中央（应位于 body 顶部 44px 行）。
4. 页面标题「让每一步研究，都有依据。」被挤压成对话栏左缘处的竖排窄条（一字一行）。

顶栏、左侧栏、footer 渲染正常（根三区布局不受影响）。

## 根因（机制层已定位）

egui 0.36 的 `Ui::allocate_ui` 会让子 Ui **继承父布局**（`allocate_ui_with_layout(desired, *self.layout(), ..)`）。
`render_body` 在 `ui.horizontal(...)`（即 `left_to_right(Align::Center)`）闭包内调用
`render_canvas` / `render_chat`，二者首行均为 `ui.allocate_ui(sz(w, h), ...)`，
于是画布 / 对话栏子 Ui 拿到**水平布局**而非 top_down：

- workspace-head 的 `allocate_ui(sz(w, 44))` 在水平+Center 布局中按 LEFT_CENTER 对齐全高 frame → 垂直居中；
- head 占满画布全宽后游标越过右缘，ScrollArea 在零宽空间渲染 → 内容一字一行竖排；
- 对话栏 header 占满 360 宽后，消息流/输入区被推到窗口右缘之外 → 残片。

与 `desktop-root-layout`（1ab3ff5f78）修复的根布局塌缩为**同一缺陷族**：当时根区已改显式矩形 +
`new_child(top_down)`，body 层漏改。同类隐患：`render_overview` 指标卡在 `ui.horizontal` 内
`allocate_ui` + `Frame::show`（Frame 同样继承布局），三段文字会横排而非纵排。

## 期望

1. `render_body` 改显式矩形切列 + `new_child(Layout::top_down(Align::Min))`（沿用根布局修复模式）。
2. 排查并修复 11 页内所有「horizontal 父级 + allocate_ui/Frame 继承布局」同类点（指标卡等）。
3. 新增无头整页几何回归测试（`Context::run_ui` 驱动真实 app，断言绘制文本坐标），锁死该缺陷族。

## 建议修复方向

变更提案 `desktop-body-layout-rd008`：F1 body 显式切列、F2 同类点排查修复、F3 ST-S15-06 无头几何回归。
