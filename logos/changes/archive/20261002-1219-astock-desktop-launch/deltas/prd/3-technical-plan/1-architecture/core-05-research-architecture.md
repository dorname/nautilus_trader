## ADDED — CLI 入口与桌面落地批次

## CLI 入口与桌面落地批次

CLI（批次 L1，crates/research-cli，bin 名 `research`）是协调器库 API 之上的薄壳：参数解析 → 类型化命令 → JSON/表格双输出。不新建线程模型——等待终态复用协调器 wait_terminal（50ms 睡眠非忙等），业务语义（幂等回执、两阶段提交、worker_bin 子进程隔离、凭证红线）全部复用既有实现，退出码契约 0（成功）/3（业务拒绝）/4（环境错误）与 worker 对齐。查询类命令只读。

桌面 GUI（批次 L2~L4，crates/research-desktop）按上文锁定的 egui＋eframe＋egui_plot 方案落地，依赖版本在首次可构建时写入 Cargo.lock。落地边界：

- GUI 线程只发类型化消息、接收不可变视图；协调器仍在独立线程持有数据库写连接；
- 引擎 Rc/RefCell 非 Send：回测计算始终由 worker_bin 子进程承载，GUI 线程不创建、
  不持有、不跨线程搬移引擎对象；
- 渲染按需重绘（仅状态变更请求帧），避免持续满帧率空转（CPU 红线）；
- AI 工作台对话为离线预设意图路由（core-09），不引入网络模型依赖；
- 分批闭环：L2 依赖与骨架、L3 研究流水线五页、L4 AI 工作台四页，每批 UT/ST＋
  reporter 与 CLI/库口径一致。
