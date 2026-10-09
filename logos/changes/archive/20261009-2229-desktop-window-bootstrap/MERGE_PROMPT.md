# 合并指令

## 变更提案
- 提案名称：desktop-window-bootstrap
- 提案目录：logos/changes/desktop-window-bootstrap/

## 提案内容

# 变更提案：desktop-window-bootstrap

> module: core | created: 2026-10-09

## 变更原因
关联 issue：**#32**（WSLg 下 research-desktop 进程存活但不创建可见窗口，
零日志零退出码）、**#1**（WSL2/WSLg 本地启动图形后端不稳定）、**#7**
（原型 ≤900px 布局与窄视口对照证据）。

本环境（WSL2 + WSLg）实测复现 #32：`LIBGL_ALWAYS_SOFTWARE=1` 启动后进程
存活 ≥8s、stdout/stderr 零字节、X 窗口树无标题窗口；强制 `WGPU_BACKEND=gl`
与 `WINIT_UNIX_BACKEND=x11` 均不改变现象。静默挂起使排障完全无从下手——
这本身（无日志无失败信号）就是必须修的问题。

## 变更类型
代码级（启动路径可观测性 + 失败显式化）+ 文档级（WSLg 启动参数与窄视口
对照证据）

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：core-S15 测试用例（新增 UT-GUI-04 窄视口布局）
- 影响的业务场景：桌面启动（研究桌面入口）
- 影响的 API：无（main.rs 启动路径内部）
- 影响的 DB 表：无
- 影响的编排测试：gui_walkthrough.rs（新增窄视口截图用例）

## 部署影响
- 是否需要部署：否
- 部署原因：桌面启动路径修正与文档，无独立部署物
- 影响环境：本地（WSL2/WSLg 开发环境）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## UI/UX 变更声明

```yaml
ui_impact: false            # 本次是否触及界面（GUI 项目才有意义）
design_system_mode: generated   # generated | fallback（fallback 时须填 design_system_fallback_reason）
design_system_fallback_reason: ""
pages: []                   # 每项 {id, prototype: core-NN-<slug>.html, description}
```

## 变更概述
1. **启动可观测**（#32/#1）：main.rs 增加启动里程碑日志（版本、显示环境
   变量、后端选择、进入事件循环前），stderr 输出；静默挂起变为有迹可循。
2. **首帧看门狗**（#32 Expected「失败时非零退出 + 明确日志」）：应用首帧
   渲染后置标志；看门狗线程超时未见到首帧则输出诊断（DISPLAY/WAYLAND/GPU
   提示与推荐参数）并以非零码退出，杜绝「进程存活但无窗口」的静默状态。
3. **WSLg 启动参数文档**：依看门狗/日志实测结论，在
   docs/research-desktop/ 写推荐启动参数与故障排查。
4. **窄视口对照证据**（#7）：gui_walkthrough 新增 880px 宽无头渲染用例
   （UT-GUI-04），断言 slim 图标栏布局生效并截图落盘
   docs/research-desktop/screenshots/，与原型 ≤900px 行为对照。


## 需要合并的 Delta 文件

### 1. deltas/test/core-S15-test-cases.md

- Delta 文件：`logos/changes/desktop-window-bootstrap/deltas/test/core-S15-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(desktop-window-bootstrap): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive desktop-window-bootstrap`。
