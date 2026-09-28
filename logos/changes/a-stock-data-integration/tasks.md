# 实现任务

## [delta] 规格变更
- [x] 产出 deltas/prd/1-product-requirements/07-astock-requirements.md — A股需求 FR-A1~A8
- [x] 产出 deltas/prd/3-technical-plan/1-architecture/04-astock-data-architecture.md — 双源架构
- [x] 产出 deltas/prd/3-technical-plan/1-architecture/diagrams/10-astock-data-flow.md — 数据流图
- [x] 产出 deltas/prd/3-technical-plan/1-architecture/diagrams/11-astock-merge-seq.md — 合并时序图
- [x] 产出 deltas/api/tickflow-klines.yaml — TickFlow K线接口契约
- [x] 产出 deltas/test/astock/astock-test-cases.md — 测试用例规格

## [code] 代码实现
- [ ] 实现 python/nautilus_trader/adapters/astock/tdx_loader.py — 通达信 .day 解析
- [ ] 实现 python/nautilus_trader/adapters/astock/tickflow_client.py — TickFlow 客户端
- [ ] 实现 python/nautilus_trader/adapters/astock/catalog_writer.py — catalog 写入与双源合并
- [ ] 编写 test_data/astock 样例与单元测试（按 astock-test-cases.md）
