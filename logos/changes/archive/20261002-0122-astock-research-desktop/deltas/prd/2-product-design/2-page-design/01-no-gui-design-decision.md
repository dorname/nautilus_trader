## MODIFIED — 决策
原无 GUI 决策继续适用于 NautilusTrader 引擎／库。a-stock 分支新增独立 Rust GUI 研究应用，适用 `core-02-research-pages.md`，页面设计不再整体跳过。

## MODIFIED — 背景
原基线为开发者通过 API／CLI 使用引擎；新增用户需求是通过 Windows／Linux 桌面完成日线选股和低频策略研究。

## MODIFIED — 理由
将桌面交互与引擎解耦，保留引擎可组合性；后台研究任务以独立进程隔离耗时计算和失败。界面、数据快照及实验元数据由应用层负责。

## MODIFIED — 影响
GUI 纳入需求、时序、接口、存储、测试与双平台交付设计。原 API／CLI 保留；无需 GUI 的用户仍可独立使用引擎。本轮仅设计，尚未实现或部署。
