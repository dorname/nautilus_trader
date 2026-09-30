# S18 原型编排检查

ST-S18-01：节点点选显示输入输出与源码映射，修改投入比例保存新流程。
ST-S18-02：预置源码检查通过，任意源码禁止演示执行；应用修复保存v2，v1不变。
ST-S18-03：设计页展示由当前流程生成的调仓周期时序图：六条生命线、消息编号①–⑥、信号日与执行日分隔线；未生成流程时显示空态引导。

检查对象是离线 HTML 原型。执行浏览器交互并断言实际状态，reporter 写入 prototype-review/ai-test-results.jsonl，使用标准id/status/duration_ms/timestamp/error字段，source标明原型检查。不得据此声明生产业务验收。
