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
- [x] 数据快照/股票池/运行/比较/计划页对接 Coordinator（类型化消息、后台任务状态、取消）
- [x] 运行页 worker_bin 子进程链路与结果视图；比较页指标表与差异清单；计划页 CSV 导出
- [x] UT/ST（页面状态机 + 协调器对接）＋ reporter；UT-S15-05/06、ST-S15-02/03 可执行性复评入 manifest

> L3 落地记录：bridge（表单→类型化 spec→Coordinator）＋ pipeline（TaskWatch
> 状态机：提交→每 500ms 轮询 get_task→终态落定即停）；静默零帧、仅活跃任务
> 期间 request_repaint_after(POLL_INTERVAL)（CPU 红线）。取消语义：GUI 长驻
> 协调器下 cancel 直达（区别于 CLI 跨命令的 ALREADY_TERMINAL——批次 L1 结论）。
> 复评结论：UT-S15-05/06 性能量测仍需参考机（保留 skip，skip 不可计为通过）；
> ST-S15-02/03 双平台 GUI 旅程因桌面应用已可启动而**具备执行条件**，本开发环境
> （WSL2 无显示）不可自动化，维持 [manual] 人工验收口径，待有显示环境执行后附
> 运行日志与截图。运行页 worker_bin 子进程链路：CoordinatorConfig 未配置
> worker_bin 时进程内执行器承载（与 CLI/测试同一路径），配置后为子进程隔离——
> GUI 不触碰引擎对象（Rc/RefCell 不跨线程），参考机性能验收时一并验证。

### 批次 L4：AI 工作台四页（S17~S20）
- [x] 离线预设意图对话引擎（意图路由 → 后台任务；项目隔离与会话持久）
- [x] 项目（需求版本）/开发（差异与不可变版本）/调试（事件单步与实验比较）/计划桥（证据边界与导出核对）四页
- [x] UT/ST（意图路由/版本冻结/证据边界纯函数与状态机）＋ reporter；S17~S20 生产用例执行口径接入

> L4 落地记录：`src/ai.rs` 离线预设意图引擎（12 意图关键词路由，Unknown 诚实
> 解释能力边界，无 LLM 无网络）；`src/workspace.rs` 工作台状态机（项目隔离多项目、
> 需求确认校验与 R 版本、设计绑定需求、代码版本不可变并冻结 R/D 引用与修订戳、
> 上游过期判定 = 引用匹配 + 冻结戳等于当前修订计数、实验记录挂协调器 task_id、
> 报告证据边界（演示检查可过 / 「正式策略验证」恒「证据不足」）、计划冻结签名
> （FNV-1a）与核对门控、确认导出产出含演示标识 CSV）；`src/app.rs` 对话面板 +
> 四子视图（意图路由自动跳转子视图，Unknown 停留），实验经真实协调器 run 执行
> （先工作台冻结校验再提交，attach_task 回填任务引用），取消复用长驻协调器直达
> 语义（批次 L3 结论）。与 S19 原型口径差异：原型确定性合成实验数值（v1 拒绝 /
> v2 收益 6.25%）为原型演示证据，生产实验 = 真实协调器 run，差异已记录于
> core-S19-test-cases.md。新增用例 UT-S17-16 / ST-S17-16 / UT-S18-15 / UT-S19-15 /
> UT-S20-14（tests/s17_s20_ai.rs 接 reporter），S17~S20 渲染旅程维持 [manual]；
> ST-S17-11~15 / ST-S18-11~14 / ST-S19-11~14 / ST-S20-11~13 原型口径不变。
