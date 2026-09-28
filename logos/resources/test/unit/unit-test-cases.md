# 单元测试用例规格（Unit Test Cases）

> 状态：基线 · 对接 `cargo test` 既有体系（logos verify 预跑）
> 变更提案：add-baseline-docs · 本文件为 OpenLogos verify 兼容格式

## 一、单元测试用例

用例 ID 规则：UT-〈域〉-〈序号〉；本基线阶段以人工文档审查方式验收。

| 用例 ID | 关联需求 | 用例描述 | 前置/步骤 | 期望结果 |
|---|---|---|---|---|
| UT-MODEL-01 | FR-003 | 订单状态机合法迁移覆盖 | 检查 crates/model 状态机实现与测试 | 状态机合法转移集内每个转移可达且事件产出正确 |
| UT-MODEL-02 | FR-003 | 非法状态迁移拒绝 | 构造状态机外迁移 | 返回错误且不产生事件 |
| UT-MODEL-03 | FR-001 | 多资产工具精度换算 | 各资产类别 Instrument（现货/期货/期权/预测市场） | 精度与最小变动单位换算正确 |
| UT-RISK-01 | FR-008 | 超额度拒单 | 订单超预设额度提交 | OrderRejected 事件产生，命令不下发 venue |
| UT-RISK-02 | FR-008 | 价格偏离拒单 | 限价超出带状范围 | 订单被拒且有事件记录 |
| UT-RISK-03 | FR-008 | 预检读取最新状态 | 预检触发时查询 Cache/Portfolio | 使用最新订单/仓位/账户状态 |
| UT-EXEC-01 | FR-003 | 命令分发路由 | 提交多场所订单命令 | 命令正确路由到目标 ExecutionClient |
| UT-EXEC-02 | FR-003, FR-011 | 执行事件回写 | venue 回报到达 | Cache 更新且事件发布到总线 |
| UT-PORT-01 | FR-009 | 成交核算 | OrderFilled 事件驱动 | 仓位与已实现/未实现盈亏增量正确 |
| UT-PORT-02 | FR-009 | 多币种换算 | 跨币种账户成交 | 换算与核算正确 |
| UT-BUS-01 | FR-011 | 三消息模式 | pub/sub、req/resp、p2p 各自收发 | 语义正确、无串扰 |
| UT-BT-01 | NFR-003 | 回测确定性 | 同一输入两次运行 | 事件流逐一致 |
| UT-BT-02 | FR-002 | 撮合仿真 | 限价/市价单进订单簿 | 成交价与数量正确 |
| UT-DATA-01 | FR-014 | 订阅分发 | 多订阅者多场所订阅 | 订阅者只收到所订阅数据 |
| UT-DATA-02 | FR-015 | 自定义数据注入 | 注册自定义类型并发布 | 数据进入引擎并被消费 |
| UT-ADV-01 | FR-013 | venue 报文解析 | 各适配器样例报文 | 解析为标准领域对象 |
| UT-ADV-02 | FR-013, FR-016 | sandbox 模板验证 | sandbox 适配器测试链路 | 作为适配器测试模板可复用 |
| UT-SER-01 | FR-004 | 事件序列化往返 | 事件序列化后反序列化 | 内容无损 |

## 二、自动化现状说明

以上用例的自动化载体为仓库既有 `cargo test`（各 crate `#[cfg(test)]`）与 python/tests；
本基线阶段的验收方式为人工审查规格与既有测试的对应关系，
后续代码类变更提案应逐步将对应用例转为自动结果（写入 test-results.jsonl）。

## 三、覆盖度校验

- [x] UT 用例覆盖全部 P0 功能需求域（model/risk/execution/portfolio/bus/backtest/data/adapter/serialization）
- [x] 每条用例标注关联需求（FR/NFR）
- [x] 用例 ID 全局唯一且符合 UT-〈域〉-〈序号〉格式

## 四、验收条件追溯

| AC ID | 验收条件 | 关联用例 |
|---|---|---|
| S01-AC-01 | 单元测试规格覆盖 P0 需求域 | UT-MODEL-01, UT-RISK-01, UT-EXEC-01, UT-PORT-01, UT-BUS-01, UT-BT-01, UT-DATA-01, UT-ADV-01, UT-SER-01 |
| S01-AC-02 | 每条用例可追溯到需求编号 | UT-MODEL-02, UT-MODEL-03, UT-RISK-02, UT-RISK-03, UT-EXEC-02, UT-PORT-02, UT-BT-02, UT-DATA-02, UT-ADV-02 |
