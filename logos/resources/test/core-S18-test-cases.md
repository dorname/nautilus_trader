# S18 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S18-11 | 设计图与说明 | 同源六节点、信号/执行时点可见；节点可定位源码 | [manual] |
| ST-S18-12 | 代码修复和版本 | 先显示差异再应用；自定义源码不能运行；保存不可变v1/v2 | [manual] |
| ST-S18-13 | 上游过期 | 需求、设计或资源更新阻止旧版本新运行；历史实验不变 | [manual] |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果——生产验收口径中本节用例标记 [manual] 排除，原型证据以 prototype-review JSONL 为准。

## GUI 落地口径（astock-desktop-launch 提案，批次 L4）

生产桌面应用（crates/research-desktop）实现本节旅程：可自动化部分（差异先显后应用、
自定义源码不可运行、不可变版本保存与上游过期判定的状态机与纯函数）由
crates/research-desktop 的 UT/ST 承载并接 OpenLogos reporter；图形渲染与交互旅程
保留人工执行（[manual] 标注不变）。原型证据保留于归档 prototype-review/。

### 生产用例执行（批次 L4 落地，crates/research-desktop tests/s17_s20_ai.rs，reporter 入账）

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S18-15 | S18-AC-01 | 版本冻结与上游过期：无需求生成设计；保存版本冻结 R/D 引用与修订戳；同源码重复保存；确认新需求后旧版本新运行 | 无需求生成设计拒绝；版本不可变（幂等保存不产生新冻结戳）；过期版本拒绝新实验且错误指向上游过期；历史实验记录不变；恢复冻结引用仍过期（冻结戳落后即过期） |

## 业务逻辑补充检查

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S18-14 | 修改需求、设计和源码草稿后切页 | 提示未确认范围，冻结输入不变；确认上游后版本过期 | [manual] |

补充检查写入 prototype-review/business-test-results.jsonl，采用相同 OpenLogos reporter 字段，仅作为原型证据，生产验收口径中标记 [manual] 排除；GUI 落地后其业务规则部分（未确认范围提示与版本过期判定）由 research-desktop UT 承载。
