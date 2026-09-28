# 端到端场景测试用例规格（Scenario Test Cases Spec）

> 状态：基线 · 端到端用户旅程级用例；执行载体为 BacktestNode/LiveExecNode + testkit
> 注：logos/resources/scenario/（API 编排 JSON）适用于服务型 API 项目；本项目为引擎，
> 端到端编排以脚本/程序形式存在，故编排规格以本文档 Markdown 形式承载于 test/ 下

## 用例规格

### ST-01 首次回测旅程（US-001，P1 画像）
**前置**：安装完成，示例数据就绪
**步骤**：EMA cross 策略 + BacktestNode 运行完整历史区间
**期望**：运行完成、订单事件链完整、绩效报告产出、重复运行结果一致
**关联**：FR-002、FR-006、NFR-003

### ST-02 多场所组合回测（US-003，P3 画像）
**前置**：两个以上场所的 catalog 数据
**步骤**：单 Strategy 同时订阅 Binance+Bybit（或任意两场所）行情，向两场所下单
**期望**：两场所订单独立完整生命周期；Portfolio 汇总两场所仓位与盈亏
**关联**：FR-001、FR-003、FR-009、FR-013

### ST-03 风控拦截端到端（US-005）
**步骤**：配置额度上限 → 提交超限订单
**期望**：RiskEngine 拒绝、OrderRejected 事件到达策略、Cache 状态一致、venue 未收到命令
**关联**：FR-008

### ST-04 回测转实盘零改动（US-002）
**步骤**：同一 Strategy 类先跑 BacktestNode（sandbox 数据）再跑 LiveExecNode（sandbox 适配器）
**期望**：策略代码零修改；两环境事件语义一致（仅时钟/撮合来源不同）
**关联**：FR-002、FR-010、FR-013

### ST-05 崩溃恢复端到端（US-007）
**步骤**：实盘 sandbox 运行中强制 kill → 重启恢复
**期望**：订单/仓位/账户状态恢复一致，后续订单可正常处理
**关联**：FR-004、FR-005、NFR-004

### ST-06 历史数据研究工作流（US-008）
**步骤**：加载数据供应商历史数据（databento/tardis 格式）→ 回放回测
**期望**：数据完整回放、时间推进正确、事件计数与源数据一致
**关联**：FR-014、FR-013

### ST-07 自定义数据融合（US-009）
**步骤**：注册自定义数据类型 → 注入信号 → 策略消费信号生成订单
**期望**：自定义数据与行情事件在同一回测中按时间序交错处理
**关联**：FR-015

### ST-08 纯 Rust 节点旅程（US-010）
**步骤**：以 Rust API（不经 Python）组装最小回测与实盘 sandbox 节点
**期望**：功能与 Python 路径等价
**关联**：FR-007、NFR-001

## 验收

- 场景用例在提案 verify 阶段随 `cargo test` / python tests 执行
- 冒烟环境复用 ST-01/04/05 的子集（见 3-deployment/02-smoke-test-spec.md）
