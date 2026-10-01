## MODIFIED — S20 原型编排检查

# S20 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 |
|---|---|---|
| ST-S20-11 | 计划账户核算 | 现金+持仓市值作总资产；预留费用；参考快照区别于实验数据 |
| ST-S20-12 | 失败阻断与过期 | 可卖不足、快照过期阻断；账户或资源修改后禁止旧计划导出 |
| ST-S20-13 | 确认导出 | 仅核对通过且确认后CSV可下载；含演示标识、版本、日期与调整数量 |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果。
