# RD-011 gui_walkthrough ST-GUI-01/02 真实 LLM 旅程结束后 SIGSEGV

- **类型**：bug / 测试稳定性
- **优先级**：P1
- **状态**：open（2026-10-12 全量多 Agent 测试记录）
- **GitHub**：[#36](https://github.com/dorname/nautilus_trader/issues/36)
- **命令**：`cargo test -p nautilus-research-desktop --test gui_walkthrough`

## 现象

默认渲染/截图路径下，`st_gui_journey_real_llm`（ST-GUI-01/02）完成截图后进程 **SIGSEGV（signal 11）**，无 `test result: ok`。

| 跑法 | 结果 |
|------|------|
| `st_gui_journey_real_llm --exact`（可渲染） | 截图落盘后 SIGSEGV（至少 2 次） |
| 整包 `--test-threads=1` | 6/6 ok；「串行/无渲染模式，跳过截图」，布局断言仍有效 |

## 复现

```bash
export RESEARCH_OCTOS_PROFILE=/root/.octos/profiles/octos.json
cargo test -p nautilus-research-desktop --test gui_walkthrough \
  st_gui_journey_real_llm -- --nocapture --exact
```

截图路径：`target/gui-walkthrough/st-gui-01-*.png`、`st-gui-02-form-filled.png`。

## 期望

旅程断言完成后进程正常退出；默认渲染路径无 SIGSEGV。无渲染串行模式不能替代默认路径修复。

## 初步判断

崩溃更像发生在 egui_kittest / harness **收尾或 drop**，而非 LLM/OUP 业务断言本身。

## 同期对照

- 原生 GUI X11 窗口可枚举，窗口像素非黑 98.3%
- `m2_swarm_integration` 15/15、`wire_real_llm` 4/4 通过
