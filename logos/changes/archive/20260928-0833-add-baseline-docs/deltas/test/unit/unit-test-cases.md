# 单元测试用例规格（Unit Test Cases Spec）

> 状态：基线 · 对接既有 `cargo test` 体系（logos.config.json verify 预跑命令）
> 本文档定义单元层的用例规格框架与代表性用例；完整执行以仓库测试为准

## 1. 测试体系现状（事实）

- 预跑命令：`cargo test`（全 workspace，OpenLogos verify 直接消费其结果）
- 覆盖位置：各 crate `#[cfg(test)]` 模块 + python/tests（Python 侧）
- 质量门：CI + pre-commit（clippy/rustfmt/codespell 等，NFR-011）

## 2. 用例规格模板

每个用例：`ID / 前置 / 步骤 / 期望 / 关联需求`

## 3. 代表性用例规格（按能力域）

### UT-MODEL 领域模型（crates/model）
- UT-MODEL-01 订单状态机合法迁移：合法迁移集内每个转移可达且事件产出正确（FR-003）
- UT-MODEL-02 非法迁移拒绝：状态机外迁移返回错误不产生事件（FR-003）
- UT-MODEL-03 工具定义：各资产类别（现货/期货/期权/预测市场）Instrument 精度与最小变动单位换算正确（FR-001）

### UT-RISK 风控引擎（crates/risk）
- UT-RISK-01 超额度拒单：订单超预设额度 → OrderRejected + 事件（FR-008）
- UT-RISK-02 价格偏离拒单：限价超出带状范围 → 拒绝（FR-008）
- UT-RISK-03 状态读取：预检读取 Cache/Portfolio 最新状态（FR-008）

### UT-EXEC 执行引擎（crates/execution）
- UT-EXEC-01 命令分发：命令正确路由到目标 ExecutionClient（FR-003）
- UT-EXEC-02 事件回写：执行事件更新 Cache 并发布到总线（FR-003/FR-011）

### UT-PORTFOLIO 组合核算（crates/portfolio）
- UT-PORT-01 成交核算：成交事件驱动仓位/盈亏增量正确（FR-009）
- UT-PORT-02 多币种换算：跨币种账户核算正确（FR-009）

### UT-BUS 消息总线（crates/common）
- UT-BUS-01 三消息模式：pub/sub、req/resp、p2p 各自语义正确（FR-011）

### UT-BACKTEST 回测（crates/backtest）
- UT-BT-01 确定性：同一输入两次运行事件流一致（NFR-003）
- UT-BT-02 撮合仿真：限价/市价单在订单簿上的成交价格与数量正确（FR-002）

### UT-DATA 数据引擎（crates/data）
- UT-DATA-01 订阅分发：订阅者只收到所订阅类型/场所的数据（FR-014）
- UT-DATA-02 自定义数据注册与注入（FR-015）

### UT-ADAPTER 适配器（crates/adapters/*）
- UT-ADV-01 消息解析：各 venue 行情/回报报文解析为标准领域对象（FR-013）
- UT-ADV-02 sandbox 适配器作为各适配器测试模板（FR-013/FR-016）

### UT-SER 事件序列化（crates/serialization）
- UT-SER-01 事件序列化/反序列化往返无损（FR-004）

## 4. 验收

`cargo test` 全绿 = 单元层验收通过（OpenLogos verify 的 pre_run 即此命令）。
新增功能类提案必须在其 delta 中扩充对应 UT 规格并在 code 任务中实现。
