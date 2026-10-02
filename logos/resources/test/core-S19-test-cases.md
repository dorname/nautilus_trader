# S19 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S19-11 | 确定性合成实验 | v1拒绝净值10000；v2持仓900、现金905、期末10625、收益6.25% | [manual] |
| ST-S19-12 | 事件调试与比较 | 筛选/单步/播放暂停、冻结源码定位、实验选择及分歧 | [manual] |
| ST-S19-13 | 验证证据边界 | v1演示失败，v2演示通过；正式验证始终证据不足；JSON冻结输入完整 | [manual] |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果——生产验收口径中本节用例标记 [manual] 排除，原型证据以 prototype-review JSONL 为准。

## GUI 落地口径（astock-desktop-launch 提案，批次 L4）

生产桌面应用（crates/research-desktop）实现本节旅程：可自动化部分（合成实验数值、
事件筛选与单步状态机、冻结输入完整性、证据边界判定的纯函数）由 crates/research-desktop
的 UT/ST 承载并接 OpenLogos reporter；演示数值断言（v1/v2 账户核算）以确定性计算
单元测试承载；播放/选择等渲染交互保留人工执行（[manual] 标注不变）。原型证据保留
于归档 prototype-review/。批次 L4 口径说明：生产实验由真实协调器 run 承载（ST-S17-16），
原型的确定性合成实验数值（v1 拒绝 / v2 收益 6.25%）为原型演示证据，不作为生产断言。

### 生产用例执行（批次 L4 落地，crates/research-desktop tests/s17_s20_ai.rs，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S19-15 | S19-AC-01 | 证据边界与实验比较：无实验生成报告；实验执行后生成报告；同输入与跨输入实验比较 | 无实验拒绝生成验证结论；演示检查可过（执行证据 = 协调器任务引用）；「正式策略验证」恒不通过且说明含「证据不足」；同输入分歧归因代码、跨输入列出代码版本与输入修订差异且不归因代码 |

## 业务逻辑补充检查

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S19-14 | 同输入修复与跨输入实验比较 | 列出需求/设计/数据/股票池/模拟器差异；跨输入不归因代码 | [manual] |

补充检查写入 prototype-review/business-test-results.jsonl，采用相同 OpenLogos reporter 字段，仅作为原型证据，生产验收口径中标记 [manual] 排除；GUI 落地后其业务规则部分（差异列举与归因边界）由 research-desktop UT 承载。
