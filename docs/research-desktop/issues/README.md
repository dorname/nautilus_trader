# nautilus-research-desktop Issue 清单

> 分支：`a-stock` · 记录日：2026-10-05（2026-10-11 增补 RD-009/010）  
> 权威原型：`logos/resources/prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html`  
> 设计规格：`core-05-ai-workspace-design.md` · 旧线框：`core-02-research-pages.md`  
> 实现：`crates/research-desktop/`

本目录为本地归档副本。**正式跟踪以 GitHub Issues 为准**（[`dorname/nautilus_trader`](https://github.com/dorname/nautilus_trader)，标签 `a-stock`）。该仓库此前关闭 Issues，已于 2026-10-05 开启。

| ID | GitHub | 标题 | 类型 | 优先级 |
|----|--------|------|------|--------|
| RD-001 | [#1](https://github.com/dorname/nautilus_trader/issues/1) | [WSL2/WSLg 本地启动图形后端不稳定](RD-001-local-launch-wslg.md) | bug / 环境 | P1 |
| RD-002 | [#2](https://github.com/dorname/nautilus_trader/issues/2) | [core-02 与 core-05 设计口径冲突](RD-002-doc-core02-vs-core05.md) | 文档 | P1 |
| RD-003 | [#3](https://github.com/dorname/nautilus_trader/issues/3) | [egui 实现与 core-05 原型视觉/交互缺口](RD-003-ui-prototype-gaps.md) | UI 保真 | P1 |
| RD-004 | [#4](https://github.com/dorname/nautilus_trader/issues/4) | [数据中心 / 股票池与原型信息架构不一致](RD-004-data-pool-ux-divergence.md) | UI / 产品 | P2 |
| RD-005 | [#5](https://github.com/dorname/nautilus_trader/issues/5) | [回测实验净值曲线仍缺失（域层未就绪）](RD-005-equity-chart-deferred.md) | 功能缺口 | P2 |
| RD-006 | [#6](https://github.com/dorname/nautilus_trader/issues/6) | [原生桌面页脚/文案仍泄漏「HTML 原型」语义](RD-006-footer-and-copy-prototype-leak.md) | 文案 | P3 |
| RD-007 | [#7](https://github.com/dorname/nautilus_trader/issues/7) | [原型 ≤900px 布局与窄视口对照证据](RD-007-prototype-narrow-viewport.md) | 原型 / 证据 | P3 |
| RD-008 | — | [主工作区 body 布局塌缩（画布竖排挤压 / 对话栏消失）](RD-008-body-layout-collapse.md) | bug / UI 布局 | P0 |
| RD-009 | [#34](https://github.com/dorname/nautilus_trader/issues/34) | [本地 octos 接线缺口](RD-009-octos-local-wiring-gaps.md) | 环境 / 基建 | P1 |
| RD-010 | [#35](https://github.com/dorname/nautilus_trader/issues/35) | [GUI 进程可起但窗口/截屏仍不可验收](RD-010-gui-window-unverified.md) | bug / 环境 | P1 |
| RD-011 | [#36](https://github.com/dorname/nautilus_trader/issues/36) | [gui_walkthrough ST-GUI 旅程后 SIGSEGV](RD-011-gui-walkthrough-sigsegv.md) | bug / 测试 | P1 |
| RD-012 | [#37](https://github.com/dorname/nautilus_trader/issues/37) | [st_m3_01 design stage 空 assistant_persisted](RD-012-m3-pipeline-empty-persisted.md) | bug / 稳定性 | P2 |

列表页：https://github.com/dorname/nautilus_trader/issues?q=is%3Aissue+label%3Aa-stock

截图目录：[`../screenshots/`](../screenshots/)（HTML 原型在窄视口下的对照；原生窗口本环境未能稳定截屏）。

## RD-003 关闭进度（2026-10-11）

GitHub #3 已关闭；修复与全量验收已完成（137通过、0失败、4跳过），变更已归档并推送 `a-stock`。2026-10-11 关闭后复查全仓未关闭 Issue 为0。逐项证据见 [RD-003 关闭证据](RD-003-closure-evidence.md)。

## 2026-10-11 本地闭环演练

- **CLI 业务闭环（S21）**：已通——导入 → 股票池 → 双运行 → 比较（cost 差异）→ 计划生成/导出/备注。工作区 `.local-loop-ws/`，产物 `导出 目录/计划.csv`。
- **octos**：项目内 `OCTOS_HOME=.local-octos` doctor 无 fatal；serve 缺 web/admin bundle → [RD-009](RD-009-octos-local-wiring-gaps.md)。
- **GUI**：需 `XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir` 才存活；截屏全黑 → [RD-010](RD-010-gui-window-unverified.md)。
- **GitHub Issue**：已提交 [#34](https://github.com/dorname/nautilus_trader/issues/34)（RD-009）、[#35](https://github.com/dorname/nautilus_trader/issues/35)（RD-010）。

## 2026-10-12 本地启动 + Agent 能力闭环

- **Agent 闭环已通**：`embed-octos-runtime` 提案验收 Gate 3.6 PASS（147 用例 100% 覆盖、0 失败）——ST-S17-17（真实 LLM 单路 turn）、ST-S17-23（三路并行 + 托管工作区产物回流）、ST-S18-17（工具可见性 + 产物落盘）实证 OUP stdio 接线全链路。
- **GUI 可视闭环已通**：`xwininfo` 可枚举「研序 · 策略研究工作区」窗口（1440×900），python-xlib 窗口级截屏非黑（亮度 22.1/255、非黑 45.3%），静默 CPU 0.9%；证据 [`../screenshots/local-loop-2026-10-12.png`](../screenshots/local-loop-2026-10-12.png)。**RD-010 / [#35](https://github.com/dorname/nautilus_trader/issues/35) 关闭**。
- **octos 接线缺口收敛**：`gh` 鉴权已恢复；bundle 缺失根因定位为 bare-binary 安装（dashboard / octos-web 经 rust_embed 构建期烘焙，运行时无法补齐）；desktop 交付不依赖 App/Admin UI，验收以 `serve` 监听 + `doctor` 无 fatal + OUP stdio 闭环为准。**RD-009 / [#34](https://github.com/dorname/nautilus_trader/issues/34) 关闭**。

## 2026-10-12 下午复测（纠正误 reopen）

- **Agent/CLI**：`ST-S17-17` ✅ 12.4s；`ST-S17-23` ✅ 405s；`ST-S17-16` ✅；`ST-S21-01` ✅。
- **误 reopen**：曾因 Wayland + 根窗口 `scrot` 全黑、以及 bundle 503，误 reopen #34/#35。
- **纠正**：按既定口径复测——X11（`env -u WAYLAND_DISPLAY`）窗口可枚举，窗口 `get_image` 非黑 **98.3%**；stdio Agent 闭环不依赖 web/admin bundle。**#34/#35 重新关闭**。证据 [`../screenshots/local-loop-2026-10-12-pm-x11.png`](../screenshots/local-loop-2026-10-12-pm-x11.png)。

## 2026-10-12 全功能 / 全流程 / 多 Agent 业务测试

**通过**：CLI S21 8/8；OUP M1 5/5；S17–S20 工作台 6/6；S15 L2/L3 17/17；wire_real_llm 4/4；**m2 Swarm 15/15**；m4 6/6；原生 GUI X11 非黑 98.3%；gui_walkthrough `--test-threads=1`（无渲染）6/6。

**失败 → Issue**：
- [RD-011](RD-011-gui-walkthrough-sigsegv.md) / [#36](https://github.com/dorname/nautilus_trader/issues/36)：默认渲染路径 ST-GUI 旅程后 SIGSEGV
- [RD-012](RD-012-m3-pipeline-empty-persisted.md) / [#37](https://github.com/dorname/nautilus_trader/issues/37)：`st_m3_01` 偶发空 `assistant_persisted`（同套件 ST-S18-16 通过）
