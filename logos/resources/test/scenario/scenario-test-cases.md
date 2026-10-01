# 端到端场景测试用例规格（Scenario Test Cases）

> 状态：基线 · 变更提案：add-baseline-docs · OpenLogos verify 兼容格式
> 说明：logos/resources/scenario/（API 编排 JSON）适用于服务型项目；本项目为引擎，
> 端到端编排以脚本/程序形式存在于仓库测试中，规格在此承载

## 一、场景测试用例

用例 ID 规则：ST-〈序号〉；`[manual]` 表示基线阶段以人工文档审查方式验收。

| 用例 ID | 关联需求 | 用例描述 | 前置/步骤 | 期望结果 |
|---|---|---|---|---|
| ST-01 | FR-002, FR-006, NFR-003 | 首次回测旅程 | EMA cross 策略 + BacktestNode 跑完整历史区间 | 运行完成、订单事件链完整、绩效报告产出、重复运行一致 |
| ST-02 | FR-001, FR-003, FR-009, FR-013 | 多场所组合回测 | 单 Strategy 订阅两场所行情并向两场所下单 | 订单生命周期独立完整；Portfolio 汇总两场所仓位盈亏 |
| ST-03 | FR-008 | 风控拦截端到端 | 配置额度上限，提交超限订单 | RiskEngine 拒绝、事件到达策略、venue 未收到命令 |
| ST-04 | FR-002, FR-010, FR-013 | 回测转实盘零改动 | 同一 Strategy 先 BacktestNode 后 LiveExecNode（sandbox） | 代码零修改；两环境事件语义一致 [manual] |
| ST-05 | FR-004, FR-005, NFR-004 | 崩溃恢复端到端 | sandbox 实盘运行中 kill -9 后重启恢复 | 订单/仓位/账户状态恢复一致，后续可继续处理 [manual] |
| ST-06 | FR-014, FR-013 | 历史数据研究工作流 | 加载供应商历史数据（databento/tardis）回放回测 | 数据完整回放、时间推进正确、事件计数一致 |
| ST-07 | FR-015 | 自定义数据融合 | 注册自定义数据类型注入信号 | 自定义数据与行情按时间序交错处理 |
| ST-08 | FR-007, NFR-001 | 纯 Rust 节点旅程 | 以 Rust API 组装最小回测与 sandbox 实盘节点 | 与 Python 路径功能等价 |

## 二、自动化现状说明

端到端自动化的载体为仓库 python/tests 与 crates 集成测试。ST-01～03、ST-06～08 已由 crates/baseline-tests 真实执行并通过 OpenLogos reporter 上报；ST-04/05 需 live 节点运行时与进程级 kill -9 注入，开发环境不可自动化，标记 [manual] 以人工审查方式验收，并复用于部署后冒烟（见 3-deployment/02-smoke-test-spec.md）。

## 三、覆盖度校验

- [x] ST 用例覆盖全部 10 条用户故事（US-001~US-010）的主旅程
- [x] 每条场景标注关联需求与验收标准
- [x] 用例 ID 全局唯一且符合 ST-〈序号〉格式

## 四、验收条件追溯

| AC ID | 验收条件 | 关联用例 |
|---|---|---|
| S02-AC-01 | 场景覆盖回测→实盘全旅程 | ST-01, ST-04, ST-08 |
| S02-AC-02 | 场景覆盖状态与恢复链路 | ST-02, ST-03, ST-05 |
| S02-AC-03 | 场景覆盖数据接入与扩展 | ST-06, ST-07 |
