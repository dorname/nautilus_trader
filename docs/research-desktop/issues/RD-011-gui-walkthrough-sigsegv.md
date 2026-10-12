# RD-011 gui_walkthrough ST-GUI-01/02 真实 LLM 旅程结束后 SIGSEGV

- **类型**：bug / 测试稳定性
- **优先级**：P1
- **状态**：fixed（2026-10-12 fix-reallm-stability-20261012 修复，待 verify 验收）
- **GitHub**：[#36](https://github.com/dorname/nautilus_trader/issues/36)
- **命令**：`cargo test -p nautilus-research-desktop --test gui_walkthrough`

## 修复（2026-10-12，fix-reallm-stability-20261012）

- **根因确认**（复现矩阵）：串行无渲染（`--test-threads=1`）235.75s 通过；渲染启用（`--exact`）必崩。触发链：渲染启用 → `snap()` 走 wgpu 软渲染（llvmpipe/LLVM JIT）→ 渲染后端在 `finish(w)` 的 `drop(w)` 及进程收尾阶段触发 glibc `nptl_deallocate_tsd` TLS 析构竞态。`walk_finished` 的 exit(0) 守卫注册于 `drop(w)` **之后**，兜不住 drop 路径的提前崩溃。
- **修法**：`finish(w)` 改为 `std::mem::forget(w)` + `walk_finished()`——泄漏 Harness/wgpu 资源，由异步 exit(0) 进程终止统一回收（与 issue_closure.rs 泄漏线程修法同源），彻底消除 drop 路径触发面。
- **验证**：渲染模式单跑 `st_gui_journey_real_llm` **119.11s ok，正常退出**（原必崩）；ST-GUI-01 三路多 Agent 派发 + ST-GUI-02 AI 整理参数断言全过。

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
