# 合并指令

## 变更提案
- 提案名称：issue-closure-rd003
- 提案目录：logos/changes/issue-closure-rd003/

## 提案内容

# 变更提案：未关闭 Issue 修复与关闭

## 变更原因
2026-10-11 查询 GitHub 全仓库未关闭 Issue（不限标签，最多 100 条），返回仅 #3（RD-003）。目标是补齐修复和关闭证据，最终关闭全部 Issue。归档状态不能代替实际验证。

## 变更类型
设计验收与证据补齐；核验发现真实缺口时补齐对应实现与测试。

## 变更范围
- 功能规格：core-05-ai-workspace-design.md「视觉保真验收口径」。
- 场景：S15 桌面布局、S17 统一工作区导航。
- 实现：research-desktop 的 theme.rs、layout.rs、app.rs、字体资源与相关 GUI 测试。
- 测试：core-S15-test-cases.md、core-S17-test-cases.md 及对应 GUI 编排。
- 本地记录：docs/research-desktop/issues/RD-003-ui-prototype-gaps.md、README.md。
- API、数据库：无契约与表结构变更。
- 需求、部署方案：沿用现行规格，不发布安装包。

## 部署影响
- 是否需要部署：否
- 部署原因：本批交付本地核验、必要修复与关闭证据，不发布安装包。
- 影响环境：本地
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否
若验证发现必须通过安装包交付才能满足要求，先修订部署决策与任务，再请求部署授权。

## 修复分组与关闭证据
全部未关闭问题归入一个变更 issue-closure-rd003，不重复实施已归档修复。

| #3 子项 | 当前静态证据 | 必须补齐的关闭证据 |
|---|---|---|
| 品牌渐变与辉光 | theme::paint_brand_mark、app 品牌入口 | 原型与原生渲染对照，渐变、圆角、文字与辉光可见 |
| 导航激活辉光 | app::nav_item 多层指示条 | 激活与非激活对照，仅激活项有指示条 |
| ≤900px 图标栏 | layout 的 900 断点与 Slim 分支 | 900、901、1200、1201 边界及窄窗口导航旅程；品牌名、分组、计数隐藏且无溢出 |
| 减少动态效果 | 规格声明当前无动画 | 检查实际绘制与重绘逻辑；存在动画则补系统偏好尊重，不能仅靠文档关闭 |
| 字体随包 | include_bytes 内嵌 Noto Sans SC | 系统字体不可用时中文仍可绘制，资源许可证与打包引用可追溯 |
| 浅色主题 | 现行规格明确深色唯一权威 | 引用已合并 RD-002 设计决策，说明此项由设计调整解决 |
| 平台近似 | 现行规格两张验收表 | 模糊、环境柔光、阴影实际行为与近似说明一致 |

## 执行顺序
1. 用户确认提案后产出验收与测试 delta，保持现有要求，不通过降低规格掩盖缺口。
2. 明确授权 merge 后合并规格；编码前列出对应 UT/ST ID。若发现缺口，同批交付业务代码、测试与 OpenLogos reporter。
3. 运行适用单元测试、GUI 编排与渲染对照，记录命令、环境、尺寸、截图与证据路径。离屏证据不能声称为原生窗口验证。
4. 明确授权 openlogos verify；通过后明确授权 archive，自动提交归档。
5. 明确授权 git push 后推送证据。按用户关闭全部 Issue 的目标，向 #3 发布逐项证据并关闭；六项证据不充分时不得关闭。
6. 重查全仓库未关闭 Issue；若有新增，按关联范围安排下一变更。

## 完成条件
六项问题及平台近似均有可审查证据；相关测试和 reporter 通过；验收、归档与远程证据提交完成；#3 为 CLOSED 且全仓库未关闭 Issue 数量为零。


## 需要合并的 Delta 文件

### 1. deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md

- Delta 文件：`logos/changes/issue-closure-rd003/deltas/prd/2-product-design/2-page-design/core-05-ai-workspace-design.md`
- 目标目录：`logos/resources/prd/2-product-design/2-page-design/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/test/core-S15-test-cases.md

- Delta 文件：`logos/changes/issue-closure-rd003/deltas/test/core-S15-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/test/core-S17-test-cases.md

- Delta 文件：`logos/changes/issue-closure-rd003/deltas/test/core-S17-test-cases.md`
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
   git add -A && git commit -m "docs(issue-closure-rd003): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive issue-closure-rd003`。
