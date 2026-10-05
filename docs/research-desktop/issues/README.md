# nautilus-research-desktop Issue 清单

> 分支：`a-stock` · 记录日：2026-10-05  
> 权威原型：`logos/resources/prd/2-product-design/2-page-design/core-05-ai-workspace-prototype.html`  
> 设计规格：`core-05-ai-workspace-design.md` · 旧线框：`core-02-research-pages.md`  
> 实现：`crates/research-desktop/`

本目录记录本地启动核验与文档/原型对照后的遗留问题。远程为 `dorname`（非 GitHub Issues），因此以仓库内 Markdown 作为 issue 载体，随 `a-stock` 分支提交。

| ID | 标题 | 类型 | 优先级 |
|----|------|------|--------|
| [RD-001](RD-001-local-launch-wslg.md) | WSL2/WSLg 本地启动图形后端不稳定 | bug / 环境 | P1 |
| [RD-002](RD-002-doc-core02-vs-core05.md) | core-02 与 core-05 设计口径冲突 | 文档 | P1 |
| [RD-003](RD-003-ui-prototype-gaps.md) | egui 实现与 core-05 原型视觉/交互缺口 | UI 保真 | P1 |
| [RD-004](RD-004-data-pool-ux-divergence.md) | 数据中心 / 股票池与原型信息架构不一致 | UI / 产品 | P2 |
| [RD-005](RD-005-equity-chart-deferred.md) | 回测实验净值曲线仍缺失（域层未就绪） | 功能缺口 | P2 |
| [RD-006](RD-006-footer-and-copy-prototype-leak.md) | 原生桌面页脚/文案仍泄漏「HTML 原型」语义 | 文案 | P3 |
| [RD-007](RD-007-prototype-narrow-viewport.md) | 原型 ≤900px 布局与 Cursor 浏览器窄视口对照证据 | 原型 / 证据 | P3 |

截图目录：[`../screenshots/`](../screenshots/)（HTML 原型在窄视口下的对照；原生窗口本环境未能稳定截屏）。
