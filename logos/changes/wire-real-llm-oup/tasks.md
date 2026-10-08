# 实现任务

## [前置] 获取远程 issue
- [x] 从 GitHub dorname/nautilus_trader 获取 #8 #9 #10 #12 #18 #19 #26–#32 详细内容
- [x] 补充到 proposal.md 的 Issue 映射表

## [delta] 修改 oup.rs：完整 turn 生命周期
- [x] 添加 `turn_wait_completed` 方法：轮询等待 turn/completed 事件
- [x] 处理 turn/error 情况：返回错误信息
- [x] 设置合理超时（120 秒）

## [delta] 修改 agent_bridge.rs：提交并等待
- [x] 添加 `submit_and_wait` 方法：提交 prompt 并等待 turn 完成
- [x] 返回 turn_id 和完成状态

## [delta] 修改 app.rs：移除 demo_task
- [x] 移除 `DemoTask` struct 和 `DemoAction` enum
- [x] 移除 `start_demo_task` / `poll_demo_task` / `cancel_demo_task` 方法
- [x] 替换 demo 路径为真实 OUP 调用（AgentBridge::submit_and_wait）

## [delta] 编写验收测试
- [x] 编写 `wire_real_llm.rs`：验证 turn/completed + 合法 turn_id
- [x] 断言无 DemoTask / DemoAction 代码路径
- [x] 测试规格 delta：`deltas/test/core-S17-test-cases.md`（新增 ST-S17-17 / ST-S17-18 / UT-S17-17）

## [delta] 合规化
- [x] 运行 `cargo test -p nautilus-research-desktop --test wire_real_llm`
- [x] 运行 OpenLogos verify 生成 test-results.jsonl
- [x] 用户确认后合并

## [code] 代码实现
- [x] app.rs：AgentWorker 后台线程接线（start/poll/cancel_agent_task + apply_agent_result）
- [x] 全量离线测试通过；ST-S17-17 / ST-S17-18 真实 LLM 验收通过
