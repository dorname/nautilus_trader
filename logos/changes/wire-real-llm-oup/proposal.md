# 变更提案：真实 LLM OUP 接线（wire-real-llm-oup）

> 模块：core；分支：a-stock；状态：新建，待实现
> 关联 issue：#8 #9 #10 #12 #18 #19 #26–#32（GitHub dorname/nautilus_trader）
> 前置变更：multi-agent-oup-integration（M1-M4 已完成）

## 变更原因

M1-M4 已完成 OUP 客户端与 Swarm 编排框架，但当前实现存在以下问题：

1. **turn 生命周期不完整**：`turn_start` 只发送请求，未处理 `turn/completed` 异步事件
2. **demo_task 残留**：`app.rs` 中存在大量 `DemoTask` / `DemoAction` 演示路径（DesignDraft、GenerateCode、UpdateData）
3. **真实 LLM 未验证**：现有测试使用 mock 或演示数据，未验证真实 LLM 调用

用户要求按 P1 优先级接线真实 LLM，验收标准：
- **必须收到 `turn/completed` 事件**（非 `turn/error`）
- **必须返回合法 `turn_id`**（UUID 格式）
- **禁止 `demo_task`**（演示任务路径）

## 变更类型

实现级变更：升级 OUP 客户端支持完整 turn 生命周期，移除 demo_task 路径，接入真实 LLM。

## 变更概述

### 当前状态（问题）

```rust
// oup.rs: turn_start 只发送请求，不等待完成
pub fn turn_start(&mut self, session_id: &str, prompt: &str) -> OupResult<Value> {
    let turn_id = uuid::Uuid::new_v4().to_string();
    self.turn_start_with_id(session_id, &turn_id, prompt)
    // 问题：未处理 turn/completed 异步事件
}
```

```rust
// app.rs: 大量 demo_task 路径
struct DemoTask { ... }
enum DemoAction { DesignDraft, GenerateCode, UpdateData }
// 问题：演示任务，非真实 LLM 调用
```

### 目标状态

1. **完整 turn 生命周期**：`turn/started` → `turn/completed`（或 `turn/error`）
2. **合法 turn_id**：UUID 格式，全局唯一
3. **真实 LLM 调用**：使用真实 LLM provider（moonshot-coding / openai 等）
4. **禁止 demo_task**：移除或禁用所有 `DemoTask` / `DemoAction` 路径

## Issue 映射（P1）

| Issue | 内容 | 状态 |
|---|---|---|
| #8 | 待补充（从 GitHub 获取） | open |
| #9 | 待补充（从 GitHub 获取） | open |
| #10 | 待补充（从 GitHub 获取） | open |
| #12 | 待补充（从 GitHub 获取） | open |
| #18 | 待补充（从 GitHub 获取） | open |
| #19 | 待补充（从 GitHub 获取） | open |
| #26–#32 | 待补充（从 GitHub 获取） | open |

**注意**：由于无法访问 GitHub 获取 issue 详细内容，以上编号需在实际执行时从远程仓库读取并补充到本提案。

## 变更范围

### 修改文件

| 文件 | 修改内容 |
|---|---|
| `crates/research-desktop/src/oup.rs` | 添加 `turn_wait_completed` 方法：轮询等待 turn/completed 事件 |
| `crates/research-desktop/src/agent_bridge.rs` | 添加 `submit_and_wait` 方法：提交并等待 turn 完成 |
| `crates/research-desktop/src/app.rs` | 移除 `DemoTask` / `DemoAction` 路径，替换为真实 OUP 调用 |

### 新增文件

| 文件 | 说明 |
|---|---|
| `crates/research-desktop/tests/wire_real_llm.rs` | 验收测试：验证 turn/completed + 合法 turn_id + 无 demo_task |

## 验收标准

### 必须满足

1. **turn/completed 事件**：每个 turn 必须收到 `turn/completed` 通知（非 `turn/error`）
2. **合法 turn_id**：turn_id 必须是合法 UUID（如 `01a1157f-e313-74a2-8f00-5a8dafc8f829`）
3. **禁止 demo_task**：代码中不得存在 `DemoTask` / `DemoAction` 相关路径

### 验证方式

```bash
# 1. 运行真实 LLM 测试
cargo test -p nautilus-research-desktop --test wire_real_llm

# 2. 验证输出包含：
#    - turn/completed 事件
#    - 合法 UUID 格式的 turn_id
#    - 无 demo_task 相关代码路径

# 3. 代码审查：确认无 DemoTask / DemoAction
grep -r "DemoTask\|DemoAction" crates/research-desktop/src/
# 应返回空（或仅注释/文档中的历史引用）
```

## 生产约束

1. **真实 LLM provider**：必须使用真实 LLM（如 moonshot-coding、openai 等），禁止 mock
2. **turn 完整性**：每个 turn 必须有完整的 started → completed 生命周期
3. **turn_id 唯一性**：每个 turn_id 必须是全局唯一 UUID
4. **错误处理**：LLM 调用失败必须返回 turn/error，不得静默失败
5. **CPU 保护**：继承 M1-M4 的 MAX_PARALLEL_DISPATCH=3 限制
6. **超时控制**：真实 LLM 调用设置合理超时（如 120 秒）

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| GitHub issue 内容不可达 | 在提案中预留编号，实际执行时从远程仓库读取补充 |
| 真实 LLM 调用延迟 | 设置合理超时（120 秒），异步通知 |
| API key 缺失或过期 | 从主 octos 数据目录复制 profile（含有效 provider 配置） |
| demo_task 残留 | 代码审查 + 测试断言禁止 demo_task 路径 |
| LLM 调用费用 | 使用最小化测试 prompt，控制调用次数 |

## 实现步骤

1. **修改 `oup.rs`**：添加 `turn_wait_completed` 轮询方法
2. **修改 `agent_bridge.rs`**：添加 `submit_and_wait` 方法
3. **修改 `app.rs`**：移除 `DemoTask` / `DemoAction`，替换为真实 OUP 调用
4. **编写 `wire_real_llm.rs`**：验收测试
5. **运行 OpenLogos verify**：生成 test-results.jsonl
6. **用户确认后合并**

## 下一步

1. 从 GitHub 获取 #8 #9 #10 #12 #18 #19 #26–#32 详细内容并补充到本提案
2. 按实现步骤修改代码
3. 编写验收测试
4. 运行 OpenLogos verify 并合并
