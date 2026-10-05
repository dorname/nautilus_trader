# 实现任务

## [delta] 规格变更
- [x] 产出 delta 文件到 `deltas/test/core-S15-test-cases.md` — MODIFIED「S15 测试用例」表格：新增 UT-S15-10（净值点转换/最近两次选取/三态横幅）与 ST-S15-05（桌面桥净值曲线读取）

## [code] 代码实现
- [ ] `bridge.rs`：`EquityPoint` + `equity_curve(run_id)` 分页拉取 + 纯函数 `equity_points()`（本批覆盖 UT-S15-10 解析断言）
- [ ] `workspace.rs`：`latest_task_experiments(n)` + `comparison_banner(a, b)` 三态文案（本批覆盖 UT-S15-10 选取/横幅断言）
- [ ] `app.rs`：`render_experiments` 新增「净值比较」面板（egui_plot 双线 + 面积填充 + 网格 + 悬停 + 横幅 + 徽章 + hint + 三态占位）
- [ ] `tests/`：UT-S15-10 用例实现（纯函数）；ST-S15-05 用例实现（合成数据链路两次运行后 equity_curve 非空、日期升序、净值为正）；均接 OpenLogos reporter
- [ ] verify 归档后：`docs/research-desktop/issues/RD-005-equity-chart-deferred.md` 标记 fixed