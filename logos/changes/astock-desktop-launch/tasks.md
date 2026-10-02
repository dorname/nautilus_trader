# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/2-scenario-implementation/core-S21-cli-research.md` — 新增 S21 CLI 研究链路场景（时序图：import/universe/run/compare/plan 子命令 → Coordinator 类型化命令；`scenario_counter.next_id` 21 由 merge 递进）
- [x] 产出 delta 文件到 `deltas/test/core-S21-test-cases.md` — S21 CLI 用例（UT：参数解析/幂等回执透传/退出码契约；ST：全链路端到端）＋「三、覆盖度校验」「四、验收条件追溯」段
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-05-research-architecture.md` — 增补 CLI 入口定位（薄壳、复用协调器线程模型）与 research-desktop crate 组件注记（批次 L2~L4 落地边界）
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/3-deployment/core-03-desktop-delivery.md` — egui/eframe 依赖获取策略（网络拉取后锁定 Cargo.lock 或 vendor 离线化）与本地构建启动前置（WSL2/双平台说明）
- [x] 产出 delta 文件到 `deltas/prd/2-product-design/2-page-design/01-no-gui-design-decision.md` — 状态行更新：GUI 由「仅设计，尚未实现」推进为「随 astock-desktop-launch 提案实现」
- [x] 产出 delta 文件到 `deltas/test/core-S17-test-cases.md`、`core-S18`、`core-S19`、`core-S20`（四个文件）— [manual] 原型编排口径更新为生产 GUI 验收口径（可自动化部分接 reporter；渲染/人工验收部分保留 [manual] 并引用原型证据）
- [x] 产出 delta 文件到 `deltas/test/core-S15-test-cases.md` — UT-S15-05/06、ST-S15-02/03 阻塞注记更新（GUI 落地后可执行性重新评估；性能量测需参考机部分保留 skip 语义）

## [code] 代码实现

### 批次 L1：CLI（crates/research-cli）
- [x] 新增 crate 骨架与子命令解析（clap 或手写；import/universe/run/compare/plan）
- [x] 子命令对接 Coordinator 类型化命令（幂等回执、等待终态、JSON/表格双输出；退出码契约 0/3/4 对齐 worker）
- [x] UT/ST 测试（用例 ID 对齐 core-S21-test-cases.md）＋ OpenLogos reporter
- [x] pre_run_command 链追加 `-p nautilus-research-cli` 并重跑 verify 确认 S21 用例入账

### 批次 L2：GUI 依赖与骨架（crates/research-desktop）
- [x] eframe/egui/egui_plot 依赖接入（网络拉取后锁定 Cargo.lock，或 vendor 离线化；记录获取方式）
- [x] 六页导航骨架＋黑色玻璃态主题（1440/1100 布局、会话状态、按需重绘不打爆 CPU）
- [x] UT（主题/布局状态/导航路由纯函数）＋ reporter

> L2 落地记录：依赖经网络 `cargo fetch` 拉取并锁定 Cargo.lock——eframe 0.36.2
> （default-features=false + wgpu/x11/wayland/accesskit）、egui 0.36.2、egui_plot 0.37.0
> （要求 egui ^0.36，与 eframe 0.36 配对）；工具链 Rust 1.98.1 满足上游 rust_version 1.95。
> 原型 backdrop-filter 真实模糊为 CSS 能力，egui 即时模式以半透明面板色＋顶部高光描边
> 近似玻璃质感（视觉验收口径已在 UT-S15-08 断言色值与对比度）。

### 批次 L3：研究流水线五页对接
- [ ] 数据快照/股票池/运行/比较/计划页对接 Coordinator（类型化消息、后台任务状态、取消）
- [ ] 运行页 worker_bin 子进程链路与结果视图；比较页指标表与差异清单；计划页 CSV 导出
- [ ] UT/ST（页面状态机 + 协调器对接）＋ reporter；UT-S15-05/06、ST-S15-02/03 可执行性复评入 manifest

### 批次 L4：AI 工作台四页（S17~S20）
- [ ] 离线预设意图对话引擎（意图路由 → 后台任务；项目隔离与会话持久）
- [ ] 项目（需求版本）/开发（差异与不可变版本）/调试（事件单步与实验比较）/计划桥（证据边界与导出核对）四页
- [ ] UT/ST（意图路由/版本冻结/证据边界纯函数与状态机）＋ reporter；S17~S20 生产用例执行口径接入
