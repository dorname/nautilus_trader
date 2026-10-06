# 合并指令

## 变更提案
- 提案名称：desktop-body-layout-rd008
- 提案目录：logos/changes/desktop-body-layout-rd008/

## 提案内容

# 变更提案：desktop-body-layout-rd008

## 变更原因

用户截图报告（issue RD-008，P0）：最大化窗口（~1750×900）下，主工作区 Overview 内容整体不可见、右侧「研究助手」对话栏消失、workspace-head 被垂直居中到 body 中央、页面标题被挤压成一字一行的竖排窄条、窗口右缘残留被裁切的文本碎片。

根因（机制层已定位）：egui 0.36 的 `Ui::allocate_ui` 让子 Ui **继承父布局**。`render_body`（app.rs:734）在 `ui.horizontal(...)`（`left_to_right(Align::Center)`）闭包内调用 `render_canvas` / `render_chat`，二者首行均为 `ui.allocate_ui(sz(w, h), ...)`，画布 / 对话栏子 Ui 因此拿到水平布局：

- workspace-head 的 `allocate_ui(sz(w, 44))` 在水平+Center 布局下被按 LEFT_CENTER 对齐全高 frame → 垂直居中（截图 y≈468 = body 正中心，与布局算法一致）；
- head 占满画布全宽后游标越过右缘，ScrollArea 在零宽空间渲染 → 页面内容一字一行竖排（截图 x≈1395 = 画布右缘 +23，与游标推进量一致）；
- 对话栏 header 占满 360 宽后，消息流 / 输入区被推到窗口右缘之外 → 「earc」残片。

与已归档 `desktop-root-layout`（1ab3ff5f78）修复的根布局塌缩为同一缺陷族：当时根区已改显式矩形 + `new_child(top_down)`，body 层漏改。同类隐患已确认一处：`render_overview` 指标卡（app.rs:1261）在 `ui.horizontal` 内 `allocate_ui` + `Frame::show`（Frame 的 content_ui 同样继承父布局，frame.rs:387），三段文字将横排而非纵排——画布修通后即刻可见。

## 变更类型

代码级修复（附带测试用例 delta：新增无头整页几何回归用例 ST-S15-06）。

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无（core-05 界面结构口径本身正确，属实现未达标）
- 影响的业务场景：S15（桌面统一工作台）
- 影响的部署方案：无
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（非 API 编排项目）
- 影响的测试用例：`logos/resources/test/core-S15-test-cases.md`（新增 ST-S15-06）
- 影响的代码：
  - `crates/research-desktop/src/app.rs` — `render_body` 显式矩形切列 + `new_child(top_down)`；指标卡等同类点改显式 `top_down`；提取 `render_root` 供无头测试驱动；新增 ST-S15-06 测试
  - `docs/research-desktop/issues/RD-008-body-layout-collapse.md` — 状态置 fixed

## 部署影响

- 是否需要部署：否
- 部署原因：本地桌面应用，无部署目标
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

F1（根因修复）：`render_body` 弃用 `ui.horizontal` + 继承式 `allocate_ui`，改为显式矩形切列（画布 `w - chat_w` / 对话栏 `chat_w`）并各建 `new_child(Layout::top_down(Align::Min))` 子 Ui——沿用根布局修复的既有模式，从机制上消除「子 Ui 继承水平布局」。

F2（同类排查）：审计 11 页全部 `horizontal*` 闭包内的 `allocate_ui` / `Frame::show` 调用点，凡内容需纵向堆叠者改为 `allocate_ui_with_layout(.., Layout::top_down(Align::Min), ..)` 或包 `ui.vertical`；已确认指标卡（app.rs:1261）一处，其余以 F3 测试全量探测。

F3（回归测试）：新增 ST-S15-06 无头整页几何回归测试——提取 `render_root(&mut self, ui)`（`eframe::Frame` 本就未使用），用 `egui::Context::run_ui` 在 1750×900 与 1100×720 两档驱动真实 app 连续渲染多帧，检查 `FullOutput.shapes` 中 `Shape::Text` 的坐标与 galley 尺寸：workspace-head 在 body 顶带、页面标题在画布内且单行（宽度阈值）、对话栏标题在右栏、指标卡三段文字纵向堆叠、无任何文本绘制越出窗口右缘。该用例锁死整个「布局继承」缺陷族，防止第三次复发。


## 需要合并的 Delta 文件

### 1. deltas/test/core-S15-test-cases.md

- Delta 文件：`logos/changes/desktop-body-layout-rd008/deltas/test/core-S15-test-cases.md`
- 目标目录：`logos/resources/test/`
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
   git add -A && git commit -m "docs(desktop-body-layout-rd008): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive desktop-body-layout-rd008`。
