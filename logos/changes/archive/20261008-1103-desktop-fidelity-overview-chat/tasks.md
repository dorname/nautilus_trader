# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/test/core-S15-test-cases.md` — 新增 UT-S15-11（欢迎正文与产物卡文案）；ST-S15-06 增加对话栏产物卡标题断言

## [code] 代码实现

> 删后续自检：六维评分 影响范围1 + 行为1 + 契约0 + 测试1 + 风险0 + 不确定0 = 3（非大任务）。概览/对话高保真与需求/开发/空态页文案同属 core-05 对齐一条能力线，拆成「主题 / 文案 / 页面」会横向切层且删后续无法各自过全量 verify，故单片。

- [x] 单切片：core-05 overview/对话与相关页高保真对齐（圆角渐变 brand-mark、accent token、欢迎正文与产物卡、多行输入、导航/步号/Slim 指标卡；需求/开发/空态页文案；UT/ST + OpenLogos reporter）（覆盖 UT-S15-11、ST-S15-06、ST-S15-07、ST-S15-08、ST-S15-09）
