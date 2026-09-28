# 变更提案：a-stock-data-integration

> module: core | created: 2026-09-28 · branch: a-stock

## 变更原因
项目当前 18 个适配器无任何中国内地市场支持（全文检索 A股/沪深/CTP/XTP 零命中）。
目标：在专属 `a-stock` 分支上建立 A股日线数据接入能力，双数据源共同维护 A股数据：
- **静态源**：通达信（TDX）人工导出的日线文件（离线补齐历史）
- **在线源**：TickFlow API（https://docs.tickflow.org/zh-Hans，免费日线额度，增量更新）

## 变更类型
需求级（新增能力域：A股数据加载层，含少量代码实现）

## 变更范围

### 新增规格文档
- `prd/1-product-requirements/07-astock-requirements.md` — A股数据需求（FR-A1~A8）
- `3-technical-plan/1-architecture/04-astock-data-architecture.md` — 双源数据架构
- `3-technical-plan/1-architecture/diagrams/10-astock-data-flow.md` — 数据流图
- `3-technical-plan/1-architecture/diagrams/11-astock-merge-seq.md` — 双源合并时序图
- `api/tickflow-klines.yaml` — TickFlow K线接口契约（OpenAPI 摘要）
- `test/astock/astock-test-cases.md` — 测试用例规格

### 新增代码（后续 code 阶段）
- `python/nautilus_trader/adapters/astock/` — A股数据加载器（双源）
  - `tdx_loader.py`：通达信日线文件解析（.day 二进制格式）
  - `tickflow_client.py`：TickFlow API 客户端
  - `catalog_writer.py`：写入 Nautilus catalog（Bar/Instrument）
- `test_data/astock/` — 样例 .day 文件与桩数据

## 部署影响
- 是否需要部署：否（库形态，无服务部署）
- 影响环境：本地开发（数据加载属用户侧运行时）
- 是否涉及数据迁移：否（新建 catalog，不动既有数据）
- 是否需要回滚预案：否（纯新增文件）
- 是否需要 smoke：否

## 变更概述
在 a-stock 分支建立"A股日线双源数据层"：通达信静态文件覆盖历史深度，
TickFlow API 覆盖增量与除权因子；两者以 catalog 为汇聚点，按 ts_event 去重合并，
统一产出 Nautilus `Bar`（`600000.SH-1-DAY-LAST-EXTERNAL`）与 `Equity` Instrument。
本提案先落规格（delta），代码实现于 SPEC_MERGED 后进行。
