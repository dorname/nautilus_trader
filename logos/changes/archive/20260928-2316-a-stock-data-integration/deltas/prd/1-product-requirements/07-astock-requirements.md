# A股数据接入需求（A-Stock Data Requirements）

> 变更提案：a-stock-data-integration · 分支：a-stock · 状态：基线

## 背景

18 个既有适配器零中国内地覆盖。A股接入首期聚焦**日线数据**（低频场景：选股/多因子/轮动，
详见 01-overview 与用户故事），执行通道（CTP/XTP/QMT 柜台）不在本提案范围。

## 功能性需求

| ID | 需求 | 优先级 |
|---|---|---|
| FR-A1 | 双源日线：通达信静态文件 + TickFlow API，共同维护全量 A股日线 | P0 |
| FR-A2 | 通达信源：解析 TDX `.day` 二进制日线（每股一文件，32 字节/记录），支持人工下载后批量导入 | P0 |
| FR-A3 | TickFlow 源：`GET /v1/klines`（symbol/period=1d/count≤10000/start_time/end_time/adjust），`x-api-key` 认证 | P0 |
| FR-A4 | 合并策略：以 catalog 为汇聚点，同标的按 `ts_event` 去重；冲突时 TickFlow（在线、含除权）优先于通达信静态导出 | P0 |
| FR-A5 | 统一输出：Nautilus `Bar`（如 `600000.SH-1-DAY-LAST-EXTERNAL`）+ `Equity` Instrument（精度 0.01，lot 100） | P0 |
| FR-A6 | 复权处理：TickFlow `adjust` 参数透传（qfq/hfq/none）；通达信源默认不复权，由 FR-A4 冲突规则收敛 | P1 |
| FR-A7 | 覆盖范围：沪深主板/创业板/科创板股票 + ETF（TickFlow 文档声明支持） | P1 |
| FR-A8 | 增量更新：TickFlow 按 `start_time` 拉增量，避免全量重拉 | P1 |

## 非功能性需求

| ID | 需求 |
|---|---|
| NFR-A1 | API Key 仅经环境变量 `TICKFLOW_API_KEY` 注入，不入库不入日志（沿用 NFR-009） |
| NFR-A2 | TickFlow 限速礼貌重试（指数退避），失败不阻塞通达信源加载 |
| NFR-A3 | `.day` 解析零第三方依赖（struct 解包），单文件 10 万行解析 < 1s |
| NFR-A4 | 合并幂等：重复运行同一数据集不产生重复 Bar |

## 用户故事（补充）

**US-A1** 作为量化研究员，我想把通达信里人工下载的日线一键导入 Nautilus catalog，
以便离线完成深度历史回测（数据早已在手，不依赖网络）。

**US-A2** 作为量化研究员，我想让 TickFlow 每日自动补齐最新日线与除权因子，
以便 catalog 始终可用而不必天天手动导文件。

## 边界（本期不做）

- 分钟线/tick（TickFlow 已有 1m~60m 能力，架构预留 period 扩展点即可）
- 实时行情订阅（五档/实时接口在 TickFlow 存在，属 DataClient 实盘范畴）
- 下单执行通道（ExecutionClient：CTP/XTP/QMT）
