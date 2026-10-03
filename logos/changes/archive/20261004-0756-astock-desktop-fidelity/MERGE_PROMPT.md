# 合并指令

## 变更提案
- 提案名称：astock-desktop-fidelity
- 提案目录：logos/changes/astock-desktop-fidelity/

## 提案内容

# 变更提案：astock-desktop-fidelity

> module: core | created: 2026-10-03

## 变更原因

目标：产品实现需要还原产品原型设计，保证高保真。

权威原型 `core-05-ai-workspace-prototype.html`（设计见 `core-05-ai-workspace-design.md`「策略研究工程台」）规定桌面应用为**统一工作台**形态：左侧唯一导航（项目切换 + 研究工作区 8 路由 + 研究资源 3 路由）、中间产物工作区、右侧常驻助手对话栏（360px）、顶部项目面包屑与版本、footer 溯源声明、专注模式；视觉为「纯黑科技 v3」（#0a0a0a 双色环境光 + 玻璃圆角卡片 + 绿指示条导航 + 等宽数字 + tag 徽章体系）。

而当前实现 `crates/research-desktop`（批次 L2~L4）是**六页平铺**形态：数据快照/股票池/运行/比较/计划/AI 工作台并列导航，对话塞在 AI 工作台页中栏内（4 个子视图纵向堆叠），无项目选择器侧栏、无右侧对话栏、无面包屑/版本 tag/专注模式/环境光/徽章组件。业务动作（协调器桥、任务状态机、版本冻结）真实可用，但**信息架构与视觉均未还原原型**。

## 变更类型

代码级（实现还原既有设计规格；设计文档 core-05 本身即为权威，不需要改设计）。

测试规格 `core-S15-test-cases.md` 中 UT-S15-07（固化「六页导航枚举」）与 UT-S15-08（布局断点语义）随实现同步更新，需产出 test delta。

## 变更范围

- 影响的需求文档：无（FR/NFR 不变；FR 中 GUI 能力集不变，仅呈现层重组）
- 影响的功能规格：无（`01-information-architecture.md` 能力域↔crate 映射不变）
- 影响的页面设计：**无 delta**——`core-05-ai-workspace-design.md` + `core-05-ai-workspace-prototype.html` 即为本次实现的权威目标，不修改设计；`core-02-research-pages.md` 线框属旧六页形态，由本次实现对齐 core-05 取代（保留原文不删，其「界面结构」通用约束——CJK 字体、1440×900/1100×720、状态文字、表格横向滚动、键盘序——继续有效）
- 影响的业务场景：S15（桌面呈现层）、S17~S20（AI 工作台路由重排，业务动作与验收语义不变）
- 影响的测试规格：`logos/resources/test/core-S15-test-cases.md`（UT-S15-07 导航枚举语义、UT-S15-08 布局断点语义）
- 影响的编排测试：ST-S17-11/16、ST-S15-04 语义不变（旅程动作不变，仅页面组织变化），无需 delta
- 影响的部署方案：无
- 影响的 API：无（bridge/worker 协议不变）
- 影响的 DB 表：无
- 影响的 smoke 测试：无

## 部署影响

- 是否需要部署：否
- 部署原因：纯桌面 GUI 呈现层重构，交付形态仍为 cargo 源码构建，无部署服务、无新依赖（egui/eframe 已有）、无配置变更
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（git revert 即可）
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: true
design_system_mode: generated
design_system_fallback_reason: ""
pages:
  - id: unified-workspace
    prototype: core-05-ai-workspace-prototype.html
    description: 统一研究工作台——左侧栏（品牌/项目选择/两组导航/离线声明）+ 主区卡片（顶栏面包屑/演示环境 tag/运行记录、工作区 canvas、右对话栏、footer 溯源）+ 专注模式；11 路由（概览/需求/设计/开发/调试/实验/验证/计划 + 数据/股票池/策略资产）
```

## 变更概述

**不改业务，只还原呈现层**：`workspace.rs`（版本冻结状态机）、`bridge.rs`（协调器协议）、`pipeline.rs`（任务状态机）保持不动；重构 `nav.rs / layout.rs / theme.rs / app.rs / ai.rs / session.rs` 六个呈现层文件 + 对应测试。

1. **导航模型**：六页枚举 → 原型 11 路由两组（研究工作区：项目概览/需求文档/策略设计/策略开发/事件调试/回测实验/验证报告/交易计划；研究资源：数据中心/股票池/策略资产），默认路由「项目概览」；现有六页的表单与 bridge 动作分别迁入对应路由（数据快照→数据中心，股票池→股票池，运行/比较→回测实验，计划→交易计划，AI 工作台四子视图→概览/开发/调试/计划 + 右侧对话常驻）。
2. **布局结构**：按原型 grid 重组——左侧栏 212px 悬浮玻璃卡片（品牌「研序」+ 项目选择/新建 + 两组导航 + 底部「离线 · 无自动下单」）；主区一张圆角玻璃大卡片，内部顶栏 65px（面包屑 研究项目/<项目名> + 「● 演示环境」tag + 「运行记录」）→ body（工作区 canvas + 右侧 360px 常驻对话栏）→ footer 28px（本地原型声明 + 「所有结果可追溯至输入与版本」）；对话栏可收起为专注模式；≤1100 宽按原型降级（178px 侧栏/320px 对话）。
3. **视觉高保真**：theme.rs 补齐原型 token——环境光（左上绿 rgba(34,197,94,.13) / 右下青 rgba(6,182,212,.11) 径向渐变近似）、文字四层级（#fafafa/#d4d4d8/#a1a1aa/#71717a）、语义色（amber #fbbf24 警告、red #f87171 错误、accent-text #4ade80）、tag/徽章/section-label（mono 大写字距）辅助函数、卡片圆角 14~16；导航激活项左缘 2px 绿指示条 + 辉光；对话气泡（用户右对齐、助手带产物卡片）、快捷提示 chips、任务条（进行中/重试）。
4. **测试同步**：UT-S15-07/08 断言与实现同步重写；对话意图路由目标页改指 11 路由；全部 `cargo test -p nautilus-research-desktop` 通过并写 OpenLogos reporter；同步更新 `logos/resources/implementation/02-research-desktop-manifest.md` 实现清单。


## 需要合并的 Delta 文件

### 1. deltas/test/core-S15-test-cases.md

- Delta 文件：`logos/changes/astock-desktop-fidelity/deltas/test/core-S15-test-cases.md`
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
   git add -A && git commit -m "docs(astock-desktop-fidelity): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive astock-desktop-fidelity`。
