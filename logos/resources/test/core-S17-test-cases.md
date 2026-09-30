# S17 原型编排检查

ST-S17-01：确认需求后六个前置角色完成，数据与研究同层；取消停止提交，重试完成。
ST-S17-02：编辑并确认新需求使旧产物过期；输入为空或数值越界不提交。

检查对象是离线 HTML 原型。执行浏览器交互并断言实际状态，reporter 写入 prototype-review/ai-test-results.jsonl，使用标准id/status/duration_ms/timestamp/error字段，source标明原型检查。不得据此声明生产业务验收。
