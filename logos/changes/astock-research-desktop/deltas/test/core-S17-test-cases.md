## MODIFIED — S17 原型编排检查

# S17 原型编排检查

本节替代旧原型检查；检查对象是离线统一HTML，不是生产API或Nautilus引擎。

| ID | 操作 | 断言 |
|---|---|---|
| ST-S17-11 | 统一入口与资源导航 | 旧入口锚点映射到同一应用，导航不丢失会话 |
| ST-S17-12 | 需求确认与项目隔离 | 空正文/越界拒绝；新项目独立；切回保留版本和消息 |
| ST-S17-13 | 对话、取消与失败重试 | 未知意图解释限制；取消不提交；模拟失败后可重试 |
| ST-S17-14 | 布局与输入安全 | 1440/1100无水平溢出；用户HTML按文本显示；专注切换可用 |

浏览器编排通过真实点击、输入和下载断言。OpenLogos reporter 使用 id/status/duration_ms/timestamp/error 字段，写入 prototype-review/unified-test-results.jsonl，source 标明原型检查，不污染生产验收结果。
