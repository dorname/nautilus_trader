# S19 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S19-11 | 确定性合成实验 | v1拒绝净值10000；v2持仓900、现金905、期末10625、收益6.25% | [manual] |
| ST-S19-12 | 事件调试与比较 | 筛选/单步/播放暂停、冻结源码定位、实验选择及分歧 | [manual] |
| ST-S19-13 | 验证证据边界 | v1演示失败，v2演示通过；正式验证始终证据不足；JSON冻结输入完整 | [manual] |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果——生产验收口径中本节用例标记 [manual] 排除，原型证据以 prototype-review JSONL 为准。

## 业务逻辑补充检查

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S19-14 | 同输入修复与跨输入实验比较 | 列出需求/设计/数据/股票池/模拟器差异；跨输入不归因代码 | [manual] |

补充检查写入 prototype-review/business-test-results.jsonl，采用相同 OpenLogos reporter 字段，仅作为原型证据，生产验收口径中标记 [manual] 排除。
