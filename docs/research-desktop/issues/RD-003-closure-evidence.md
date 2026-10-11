# RD-003 修复与关闭证据

记录时间：2026-10-11。关联变更：issue-closure-rd003。GitHub：[问题 #3](https://github.com/dorname/nautilus_trader/issues/3)。

## 六项问题结论

| 问题 | 修复或设计决策 | 证据 |
|---|---|---|
| 品牌标识 | 29px、9px 圆角、145° 渐变；补齐多层外圈辉光，中文居中 | UT-S15-12；五档 egui/原型截图 |
| 激活导航 | 2px 指示条与三层透明辉光；修正透明色构造；非激活无指示条 | ST-S15-16 绘制图元断言与截图 |
| 900px 断点 | 原生最小窗口 880×720，900 为 65px 图标栏；901/1200 为窄档；1201 为舒适档；图标档隐藏品牌名、分组与计数，图标可导航 | UT-S15-08、ST-S15-16、UT-GUI-04/05 |
| 减少动态效果 | 启动读取系统设置，未知默认减少；运行记录中可显式选择；静止任务指示器、关闭控件过渡与滚动动画，保留状态文字、取消和轮询 | UT-GUI-06 的真实任务条与交互测试；motion.rs |
| 中文字体随包 | Noto Sans SC 编译期内嵌，系统字体不可用仍有中文实际字形；附 Adobe 版权和 OFL 1.1 许可证 | UT-S15-12、assets/OFL.txt、theme::setup_embedded_fonts |
| 浅色主题 | 已按 RD-002 合并决策取消浅色要求，core-05 深色为唯一权威 | core-05「关闭声明」与 desktop-issue-docs-rd002-rd004 归档 |

## 渲染对照

| 视口 | egui 离屏图 | HTML 原型图 |
|---|---|---|
| 880×720 | [egui](../screenshots/rd003/egui-880.png) | [原型](../screenshots/rd003/prototype-880.png) |
| 900×900 | [egui](../screenshots/rd003/egui-900.png) | [原型](../screenshots/rd003/prototype-900.png) |
| 901×900 | [egui](../screenshots/rd003/egui-901.png) | [原型](../screenshots/rd003/prototype-901.png) |
| 1200×900 | [egui](../screenshots/rd003/egui-1200.png) | [原型](../screenshots/rd003/prototype-1200.png) |
| 1201×900 | [egui](../screenshots/rd003/egui-1201.png) | [原型](../screenshots/rd003/prototype-1201.png) |

环境：Linux 5.15.167.4-microsoft-standard-WSL2，egui 0.36.2 与 egui_kittest wgpu 离屏渲染；原型经 Chromium 153 无头截图。截图为实际原生 egui 绘制证据，不是可见窗口截图，不证明 Windows 安装包或双平台部署通过。字体测试强制只用内嵌字体。原型布局计算值见 prototype-layout.json，五档均无页面水平溢出。

平台边界：玻璃模糊以半透明面板和描边近似，环境柔光以网格近似，CSS 阴影以分层透明图元近似；不声称像素级等同。其余页面字距与控件高度的差异不作为这六项问题已实现像素级一致的声明。

## 测试与流程

- issue_closure：4 项通过，包含 UT-S15-12、ST-S15-16、UT-GUI-05/06。
- motion_keeps_task_controls_and_polling：通过，检查真实运行中任务条在两种偏好下的绘制、取消与轮询。
- s15_desktop_l2：3 项通过，包含 UT-S15-08。
- gui_walkthrough 的 ut_gui 子集：4 项通过，包含 UT-GUI-04。
- 所有新增测试包含共享 OpenLogos reporter。
- 首次全量验收因 Moonshot 七天额度耗尽（HTTP 403）失败，不能记为通过。为完整复验，测试遵循既有 RESEARCH_OCTOS_PROFILE 显式选择机制，使用本机已配置 MiniMax-M3 的临时配置；不改写用户默认配置、不跳过真实 LLM 断言。MiniMax 单次真实 turn 检查已通过；最终全量验收结果待追加。
- GitHub 关闭状态：待完成全量验收、归档及推送后关闭。

## 平台偏好依据

Windows 使用 [Microsoft SystemParametersInfoW 的 SPI_GETCLIENTAREAANIMATION](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow)；Linux GNOME 读取 enable-animations，参考 [GNOME 动画设置](https://gnome.pages.gitlab.gnome.org/gtk/gtk4/property.Settings.gtk-enable-animations.html)。读取失败或非支持桌面环境默认静止，可通过显式开关调整。Windows 分支尚未在 Windows 机器运行，本次证据不把它记为 Windows 运行验收。
