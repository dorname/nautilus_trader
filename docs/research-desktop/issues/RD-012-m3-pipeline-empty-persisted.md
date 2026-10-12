# RD-012 st_m3_01_pipeline_dispatch 偶发：design stage assistant_persisted 为空

- **类型**：bug / 真实 LLM 稳定性
- **优先级**：P2
- **状态**：fixed（2026-10-12 fix-reallm-stability-20261012 修复，待 verify 验收）
- **GitHub**：[#37](https://github.com/dorname/nautilus_trader/issues/37)
- **命令**：`cargo test -p nautilus-research-desktop --test m3_pipeline_integration`

## 修复（2026-10-12，fix-reallm-stability-20261012）

- **根因**：`turn_wait_completed` 等到 `turn/completed` 即返回；服务端先推 completed、后落盘再推 `assistant_persisted` 时，迟到的 persisted 通知被错过（时序竞态，与同族 turn_id 回显不稳定同源）。
- **修法**：`oup.rs` 新增 `drain_persisted_grace`（独立自由函数，UT 可 channel 注入）——completed 到达但文本空时进入 **3s 宽限窗口**继续收割迟到的 persisted（同步收集 tool 事件）；`recv_timeout`/`Disconnected` 路径补齐遗漏的 `parse_tool_event`。空产物失败消息细化为「turn/completed 已到但 assistant_persisted 宽限窗口内未到达（或模型空回复）」，与 turn/error、Cancelled 可区分（期望 2）。
- **覆盖用例**：新增 UT-S18-19（宽限收割迟到 persisted + tool 事件不丢）、UT-S18-20（期满及时返回空 / 通道断开提前结束）。
- **回归**：m3 全套件 5/5（含 st_m3_01、ST-S18-16，真实 LLM，366s）通过。
- **未做（留后续提案）**：m3 契约对齐 m2 产物文件型（ManagedFile 回流）——pipeline 为链式文本传递语义；若宽限修复后仍偶发再立项。

## 现象

Pipeline 第一 stage（design）偶发 Failed：`stage 无文本产物（assistant_persisted 为空）`；下游 code-check Cancelled（「上游 stage 失败，未派发」）。

失败样例（同次套件）：

```
turn_id=85ce7088-dda8-4081-9b29-1ef8981a934c
design: Failed, assistant_persisted 为空
code-check: Cancelled（上游失败未派发）
```

同套件：`st_m3_02/03/04` ok，**`st_s18_16_pipeline_injects_upstream_output` ok**（4 passed; 1 failed）。

## 复现性

| 跑法 | 结果 |
|------|------|
| 全量 `--test-threads=1` | st_m3_01 **失败**一次 |
| 随后 `--exact st_m3_01_pipeline_dispatch` | **通过**（150.6s） |

判定为真实 LLM / 产物抽取**偶发**，非确定性必现。

## 期望

1. design stage 在 turn/completed 后稳定抽出非空 `assistant_persisted`（或文件产物回流）
2. 模型偶发空回复时，测试侧可诊断重试/诚实失败分类
3. 与已通过的 ST-S18-16 对齐稳定性

## 关联

历史已关：GitHub #11（Pipeline 未等上游）、#18（须用 assistant_persisted）。本条为稳定性回归跟踪。
