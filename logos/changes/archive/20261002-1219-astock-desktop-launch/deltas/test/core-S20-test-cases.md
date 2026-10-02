## MODIFIED — S20 原型编排检查

# S20 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 | 验收方式 |
|---|---|---|---|
| ST-S20-11 | 计划账户核算 | 现金+持仓市值作总资产；预留费用；参考快照区别于实验数据 | [manual] |
| ST-S20-12 | 失败阻断与过期 | 可卖不足、快照过期阻断；账户或资源修改后禁止旧计划导出 | [manual] |
| ST-S20-13 | 确认导出 | 仅核对通过且确认后CSV可下载；含演示标识、版本、日期与调整数量 | [manual] |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果——生产验收口径中本节用例标记 [manual] 排除，原型证据以 prototype-review JSONL 为准。

## GUI 落地口径（astock-desktop-launch 提案，批次 L4）

生产桌面应用（crates/research-desktop）实现本节旅程：可自动化部分（计划账户核算
公式、失败阻断与过期判定、导出核对与标识列生成的纯函数）由 crates/research-desktop
的 UT 承载并接 OpenLogos reporter，与 S15 库验收口径（core-S15-test-cases.md）同源
复用；确认对话框与下载交互保留人工执行（[manual] 标注不变）。原型证据保留于归档
prototype-review/。
