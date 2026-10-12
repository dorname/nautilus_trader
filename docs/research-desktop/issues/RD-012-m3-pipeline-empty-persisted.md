# RD-012 st_m3_01_pipeline_dispatch 偶发：design stage assistant_persisted 为空

- **类型**：bug / 真实 LLM 稳定性
- **优先级**：P2
- **状态**：open（2026-10-12 全量多 Agent 测试记录）
- **GitHub**：[#37](https://github.com/dorname/nautilus_trader/issues/37)
- **命令**：`cargo test -p nautilus-research-desktop --test m3_pipeline_integration`

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
