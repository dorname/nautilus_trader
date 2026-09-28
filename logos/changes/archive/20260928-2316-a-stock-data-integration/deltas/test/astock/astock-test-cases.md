# A股数据接入测试用例规格（A-Stock Test Cases）

> 变更提案：a-stock-data-integration · OpenLogos verify 兼容格式

## 一、测试用例

| 用例 ID | 关联需求 | 用例描述 | 前置/步骤 | 期望结果 |
|---|---|---|---|---|
| UT-AST-01 | FR-A2 | .day 文件解析正确 | 构造 3 记录的样例 .day 文件 | OHLCV 与日期逐字段正确（价格÷100） |
| UT-AST-02 | FR-A2 | 坏文件容错 | 截断/长度非32倍数的文件 | 跳过并告警，不抛异常 |
| UT-AST-03 | FR-A3 | TickFlow 200 解析 | mock KlinesResponse | Bar 字段映射正确（Price精度2/Quantity/UnixNanos） |
| UT-AST-04 | FR-A3 | TickFlow 错误处理 | mock 401/429/404 | 401 报凭证错；429 退避重试；404 记录缺失 |
| UT-AST-05 | FR-A4 | 时间戳去重合并 | 两源含重叠 ts_event | 重叠条目仅存一份（TickFlow 版本） |
| UT-AST-06 | FR-A4 | 冲突优先级 | 同 ts_event 不同 close | 保留 TickFlow 值 |
| UT-AST-07 | FR-A5 | BarType 规范 | 合并产物 | 形如 600000.SH-1-DAY-LAST-EXTERNAL |
| UT-AST-08 | FR-A5 | Instrument 输出 | 合并产物 | Equity：precision 2、lot_size 100 |
| UT-AST-09 | FR-A8 | 增量拉取 | catalog 已有至 T 日 | 请求 start_time=T+1，不重拉历史 |
| UT-AST-10 | NFR-A1 | API Key 不泄漏 | 全链路日志检查 | 无明文 key |
| UT-AST-11 | NFR-A4 | 合并幂等 | 同数据集跑两遍 | 第二遍 catalog 无新增无变更 |
| ST-A1 | FR-A1, US-A1 | 通达信全量建库 | 样例目录→catalog | 回测可读全部 Bar，报告条数一致 |
| ST-A2 | FR-A1, US-A2 | 双源共建+增量 | tdx 全量+tickflow 增量 | 重叠按 FR-A4 收敛；总条数=并集 |

## 二、自动化现状说明

UT-AST-01/02 可离线自动化（样例文件）；UT-AST-03~06/09/11 以 mock/stub 自动化；
ST-A1/A2 为端到端（本地 catalog 断言）。基线阶段以 manual-doc-review 记录，
code 阶段转为 pytest 自动结果。

## 三、覆盖度校验

- [x] 用例覆盖 FR-A1~A8 全部功能性需求
- [x] 用例覆盖 NFR-A1/A2/A4
- [x] 用例 ID 全局唯一（UT-AST-*/ST-A* 前缀，与既有 26 例不冲突）

## 四、验收条件追溯

| AC ID | 验收条件 | 关联用例 |
|---|---|---|
| S10-AC-01 | 双源解析与映射正确 | UT-AST-01, UT-AST-03 |
| S10-AC-02 | 合并与幂等正确 | UT-AST-05, UT-AST-06, UT-AST-11 |
| S10-AC-03 | 产出符合 Nautilus 规范 | UT-AST-07, UT-AST-08 |
| S10-AC-04 | 端到端建库可用 | ST-A1, ST-A2 |
