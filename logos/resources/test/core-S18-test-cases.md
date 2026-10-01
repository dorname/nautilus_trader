# S18 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 |
|---|---|---|
| ST-S18-11 | 设计图与说明 | 同源六节点、信号/执行时点可见；节点可定位源码 |
| ST-S18-12 | 代码修复和版本 | 先显示差异再应用；自定义源码不能运行；保存不可变v1/v2 |
| ST-S18-13 | 上游过期 | 需求、设计或资源更新阻止旧版本新运行；历史实验不变 |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果。

## 业务逻辑补充检查

| ID | 操作 | 断言 |
|---|---|---|
| ST-S18-14 | 修改需求、设计和源码草稿后切页 | 提示未确认范围，冻结输入不变；确认上游后版本过期 |

补充检查写入 prototype-review/business-test-results.jsonl，采用相同 OpenLogos reporter 字段，仅作为原型证据。
