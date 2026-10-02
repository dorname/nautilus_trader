# 变更提案：astock-desktop-launch（本地启动——CLI + 桌面 GUI）

> module: core | created: 2026-10-02

## 变更原因

astock-research-desktop 提案已交付研究后端全链路（S11~S15 协调器库 + worker 子进程，
verify Gate 3.5/3.6 PASS），但交付物是**库形态，无本地启动入口**：

- 生产 GUI 未实现——架构（core-05）已锁定 egui＋eframe＋egui_plot 方案，但 eframe 依赖
  不在离线缓存，crates/research-desktop 从未创建（记录于上一提案批次 6 环境阻塞清单）；
- CLI 入口缺失——唯一 bin `research-run-task` 是被协调器拉起的子进程，不能独立驱动
  「导入 → 股票池 → 回测运行 → 比较 → 计划导出」全链路；
- 用户明确提出「本地启动」，并选定 **GUI + CLI 一起提案、先 CLI 后 GUI 分批闭环**。

AI 工作台（S17~S20）不依赖外部模型：core-09 明确「对话为离线预设意图交互，不连接
真实模型」，因此 GUI 六页（研究流水线 + AI 工作台）均可生产化，无 LLM 接入范围。

## 变更类型

设计级（原型／场景／部署设计已在主文档，本提案落地实现，并新增 CLI 场景、
更新 GUI 验收口径；不改动 API／DB 契约）。

## 变更范围

- 影响的需求文档：core-08-research-requirements.md（不改动——CLI 承载既有 FR-R 系列）、
  core-09-ai-workspace-requirements.md（不改动——离线预设意图语义原样落地 GUI）
- 影响的功能规格：01-no-gui-design-decision.md（状态行更新：GUI 由「仅设计」推进为「实现中」）
- 影响的业务场景：新增 **S21（CLI 研究链路）**；S17~S20 场景实现文档增补「GUI 落地映射」注记
- 影响的部署方案：core-03-desktop-delivery.md（egui/eframe 依赖获取策略：网络拉取后锁定
  Cargo.lock 或 vendor；本地开发启动前置）
- 影响的 API：无（Coordinator 类型化命令面不变，CLI/GUI 均为既有库 API 的前端载体）
- 影响的 DB 表：无（research SQLite schema 不变）
- 影响的编排测试：scenario/core-research-orchestration.json 不变；新增 S21 编排参照
- 影响的测试用例：
  - 新增 `core-S21-test-cases.md`（CLI 用例，承载 S11~S15 全链路的端到端验收）
  - 更新 `core-S17~S20-test-cases.md`（[manual] 原型编排口径 → 生产 GUI 验收口径，
    原型证据保留引用；可自动化部分接 reporter，渲染类用例诚实标注人工验收）
  - 更新 `core-S15-test-cases.md` 的 UT-S15-05/06、ST-S15-02/03（GUI 落地后重新评估
    可执行性；性能量测仍需参考机的部分保持 skip 语义）
- 影响的 smoke 测试：core-desktop-smoke-test-cases.md 不改动（smoke 属部署后，
  本提案不执行部署）

## 部署影响

- 是否需要部署：**否**
- 部署原因：交付物是本地启动工具（CLI）与桌面应用（GUI），由用户在本机直接构建运行，
  不涉及 staging／生产环境发布。core-03 的双平台安装包交付与 smoke 留待后续部署级提案。
- 影响环境：本地
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

**批次 L1（CLI 先行）**：新增 crate `crates/research-cli`（bin 名 `research`），子命令
直接驱动 `Coordinator`：`import`（行情/主档/财务/公司行为/日历/规则暂存导入）、
`universe`（预览/保存）、`run`（提交运行与网格、等待终态、查看结果）、`compare`、
`plan`（生成/导出/备注）。CLI 为薄壳：参数解析 → 类型化命令 → 输出 JSON/表格，
业务语义全部复用协调器既有实现（含幂等回执、两阶段提交、worker_bin 子进程隔离、
TICKFLOW_API_KEY 凭证红线）。配套 core-S21 场景与测试用例（UT/ST + reporter）。

**批次 L2~L4（GUI 分批）**：新增 crate `crates/research-desktop`（egui＋eframe＋egui_plot，
架构 core-05 锁定方案）。L2 依赖接入与六页导航骨架（黑色玻璃态主题、1440/1100 布局、
会话状态）；L3 研究流水线五页对接协调器（数据快照/股票池/运行/比较/计划，后台任务
状态与取消，GUI 只发类型化消息、接收不可变视图）；L4 AI 工作台四页（S17~S20 离线
预设意图对话、项目隔离、版本冻结、证据边界、计划核对导出）。每批业务代码＋UT/ST＋
reporter 闭环；引擎 Rc/RefCell 非 Send 约束下，回测仍由 worker 子进程承载，
GUI 线程不触碰引擎对象。

**验收红线不变**：不能打爆 CPU（GUI 渲染帧率按需重绘、后台任务走协调器线程模型）；
凭证只经数据导入子进程；SQLite 只由协调器写。
