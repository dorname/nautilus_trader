# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/test/core-S15-test-cases.md` — 新增 UT-S15-11（欢迎正文与产物卡文案）；ST-S15-06 增加对话栏产物卡标题断言

## [code] 代码实现

- [x] `theme.rs`：圆角渐变 brand-mark；`--accent-soft` / `--accent-border` token
- [x] `workspace.rs` / `nav.rs`：欢迎正文与产物卡文案与原型对齐
- [x] `app.rs`：产物卡结构、多行输入、导航色、步号、Slim 指标卡；需求页页头徽章 / 默认草稿 / 万元字段 / 下载文档
- [x] 开发页对齐 `#develop`；空态回测 / 验证页头主按钮 / 计划徽章 / 策略资产「打开编辑器」/ 调试空态副标题
- [x] UT-S15-11 + ST-S15-06/07/08/09；OpenLogos reporter 写入 `logos/resources/verify/test-results.jsonl`
