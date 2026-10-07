# 合并指令

## 变更提案
- 提案名称：desktop-fidelity-overview-chat
- 提案目录：logos/changes/desktop-fidelity-overview-chat/

## 提案内容

# 变更提案：desktop-fidelity-overview-chat

> module: core | created: 2026-10-06

## 变更原因

对照权威原型 `http://127.0.0.1:8766/core-05-ai-workspace-prototype.html#overview`（1440 档截图）与 `crates/research-desktop` 实现，概览页与工作台外壳仍有可核对的高保真缺口。RD-003/RD-006/RD-008 已归档的布局与辉光近似之后，下列项仍与原型不一致：

1. 品牌 mark 为未圆角四顶点 Mesh（原型 `.brand-mark` 29×29、圆角 9、145° 渐变、字色 `#03130a`）。
2. 欢迎对话产物卡只渲染路由标题「需求文档」，缺少原型 `title`/`desc`（「先写下你的研究想法」/「需求草稿 · 等待确认」）与文件图标。
3. 欢迎正文缺第二段与换行；输入区为单行而非原型 64px 多行 textarea（↵ 发送 / Shift+↵ 换行）。
4. 导航激活底/描边未使用 `--accent-soft` / `--accent-border` 原色；研究路径步号描边过弱。
5. `@media(max-width:900px)` 指标卡应只保留第一张，实现始终三张。

本变更为代码级高保真对齐（设计已由 core-05 规定，不改产品规格正文）。WSLg 原生窗口截图仍受 RD-001 限制，本批以无头 `Context::run_ui` 文本图元 + 纯函数断言为自动化证据。

## 变更类型

代码级修复（附带测试用例 delta：UT-S15-11、ST-S15-06 扩展断言）。

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无（core-05 已规定上述视觉/文案；实现未达标）
- 影响的业务场景：S15（桌面统一工作台）
- 影响的部署方案：无
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的测试用例：`logos/resources/test/core-S15-test-cases.md`（新增 UT-S15-11；ST-S15-06 增加产物卡文案在对话栏的断言）
- 影响的代码：
  - `crates/research-desktop/src/theme.rs` — 圆角渐变 brand-mark、`--accent-soft`/`--accent-border` token
  - `crates/research-desktop/src/app.rs` — 产物卡结构、输入区、导航色、步号、Slim 指标卡
  - `crates/research-desktop/src/workspace.rs` — 欢迎正文与原型对齐
  - `crates/research-desktop/src/nav.rs` — 产物卡默认/欢迎文案常量
  - `crates/research-desktop/tests/s15_desktop_l2.rs` / `s15_desktop_l3.rs`

## 部署影响

- 是否需要部署：否
- 部署原因：本地桌面应用，无部署目标
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true
design_system_mode: generated
design_system_fallback_reason: ""
pages:
  - id: overview
    prototype: core-05-ai-workspace-prototype.html
    description: 项目概览 + 工作台外壳（品牌、导航激活、对话产物卡、输入区、研究路径步号、Slim 指标卡）
```

## 变更概述

对照 core-05 overview 原型，把品牌 mark 改成圆角 9 的 145° 渐变块；对话产物卡改为标题+说明+图标（欢迎卡用原型固定文案）；欢迎消息补全第二段；输入区改为 64px 多行并保留 ↵ 发送；导航激活与步号描边改用原型 accent 透明色；≤900 宽只显示第一张指标卡。用 UT-S15-11 锁文案常量，用 ST-S15-06 无头几何确认产物卡标题落在对话栏。


## 需要合并的 Delta 文件

### 1. deltas/test/core-S15-test-cases.md

- Delta 文件：`logos/changes/desktop-fidelity-overview-chat/deltas/test/core-S15-test-cases.md`
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
   git add -A && git commit -m "docs(desktop-fidelity-overview-chat): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive desktop-fidelity-overview-chat`。
