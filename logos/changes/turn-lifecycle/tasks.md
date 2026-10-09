# 实现任务

## [delta] 规格变更
- [x] deltas/test/core-S17-test-cases.md：注册 UT-S17-20 / ST-S17-22（取消与中断口径）

## [code] 代码实现

- [x] OupClient::turn_interrupt（turn/interrupt RPC 封装）
- [x] AgentBridge::interrupt_turn + AgentError::NoActiveTurn（按 active_turn_id 真实中断；无活跃 turn 拒绝）
- [x] SwarmBridge::cancel_all 重写（按 turn_id 逐 agent interrupt；发出才 Cancelled；未发出 Failed + CancelIncomplete Err；mark_cancel_outcomes 纯函数归约）
- [x] AgentWorker/SwarmWorker 请求枚举化（Submit/Cancel、Dispatch/Cancel），忙期真实 interrupt；GUI 任务条「取消」接线（先真实中断再本地放弃）
- [x] UT-S17-20 无 LLM 判定矩阵 + ST-S17-22 真实 LLM interrupt 终态断言（reporter 入账，m2 套件 10/10 通过）
- [x] st_m2_04_cancel_all 重写为 ST-S17-22 新口径
- [x] 超时预算校准（只放宽等待、不动断言）：DEFAULT_TURN_TIMEOUT 120→420s、SWARM_WORKER_TIMEOUT 300→600s、ST-S17-19 180→480s、wire/m4 显式预算 120/180→420s、GUI 泵帧 420→700/500s（moonshot 拥塞期实测尾延迟 270s+）
