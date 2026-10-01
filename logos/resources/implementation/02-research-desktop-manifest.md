# 研究桌面实现清单（astock-research-desktop）

> 状态：批次 3a 完成（S11+S12+S13信号内核）· 真相源：crates/research-domain、crates/research-testkit
> 上游规格：core-05-research-architecture.md / core-research-contracts.yaml / core-01-research-storage.sql / core-S11/S12/S13-test-cases.md

## 批次 1：S11 数据维护（2026-10-01）

**覆盖用例**：UT-S11-01～UT-S11-05、ST-S11-01～ST-S11-02（7/7 pass，见 logos/resources/verify/test-results.jsonl）

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| nautilus-research-testkit | crates/research-testkit | OpenLogos reporter（JSONL 追加、run_id/platform/proposal 溯源、panic 记 fail 后重抛） |
| 协议类型 | crates/research-domain/src/protocol.rs | ImportSpec/TaskRef/TaskView/SnapshotRef 等契约类型，S11 范围 |
| 任务状态机 | src/task.rs | queued→running→succeeded/failed/cancelled、cancelling、interrupted；终态不可回退 |
| 行情校验 | src/quotes.rs | 暂存 CSV 逐行校验（非法日期/负成交量/high<low/重复键→整批拒绝带行号） |
| Parquet 分区 | src/parquet_io.rs | 十进制定点 OHLC、UTC 纳秒 available_at；ingested_at 不入哈希内容 |
| 对象存储 | src/objects.rs | SHA256 内容寻址、fsync+原子改名、只读对象、孤儿回收 |
| 元数据 | src/store.rs | DDL 经 include_str! 单源引用规格 SQL；单写连接 WAL + 只读连接；busy 3s→BUSY |
| 协调器 | src/coordinator.rs | ImportData/GetTask/CancelTask/ListSnapshots/分页行情；两阶段提交；幂等回执 |

**CPU 红线落实**（验收：不打爆 CPU）：
- 协调线程 `crossbeam::channel::select!` 阻塞等待命令与完成通知，空闲零占用，无轮询空转；
- 计算线程随任务生灭；取消经原子标记在 32 行批次边界响应；
- 调用方等待终态（wait_terminal）固定 50ms 睡眠，不做忙等待。

**幂等与一致性**：
- command_receipt 与 queued 任务同事务；同键同哈希返回原 TaskRef，不同载荷 IDEMPOTENCY_CONFLICT；
- 分区→fsync→SHA256→原子改名→单事务落库；崩溃注入点（提交前）验证无部分快照；
- 相同内容重复导入复用对象与快照（manifest 哈希确定性，不含时间戳/UUID）。

**verify 配置修正**：logos.config.json 的 pre_run_command 已由固定 pass 占位改为真实执行
`rm -f test-results.jsonl && cargo test -p nautilus-research-domain --offline`（规格 core-09 要求）。

## 批次 2：S12 股票池（2026-10-01）

**覆盖用例**：UT-S12-01～UT-S12-05、ST-S12-01～ST-S12-02（7/7 pass；连同 S11 回归共 14/14，见 test-results.jsonl）

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| 辅助数据 | src/auxiliary.rs | 主档/财务暂存 CSV 逐行校验（available_at 必填、主键冲突整批拒绝）、规范化 JSON 内容对象 |
| 规则引擎 | src/universe.rs | RuleAST（AND 根 ≤20 子节点、OR 组、深度≤2）静态校验 + 三态评估（命中/不满足/未知） |
| 协议类型 | src/protocol.rs | UniverseSpec/SaveUniverseSpec/UniverseRef/UniversePreview/RowsTable/RowsPage |
| 协调器扩展 | src/coordinator.rs | PreviewUniverse 异步任务、SaveUniverse 同步命令、QueryRows 三桶分页、严格预检 |
| 存储扩展 | src/store.rs | commit_task_result 通用产物提交、save_universe_with_receipt 单事务保存、get_universe |
| 清单扩展 | src/manifest.rs | manifest.auxiliary 引用（master/financial），能力/限制按实际内容生成 |

**时点纪律落实**：
- 财务只取 available_at ≤ as_of 的报告（同报告期取最大修订号），未来报告条件判未知（UT-S12-01）；
- 主档按 as_of 当时状态判断在册，退市标的历史池保留（UT-S12-02）；
- 成交额缺失判未知，禁止 close×volume 替代（UT-S12-05；QuoteRow.amount_cny 改 Option，Parquet 列可空）；
- 固定池形成日晚于回测开始日严格预检拒绝并指出形成日（UT-S12-04，precheck_run_universe 供 S13 复用）。

**一致性落实**：
- 预览产物为内容寻址 JSON（行按标的排序，哈希确定性）；保存校验 input_hash+preview_hash 双一致，否则 STALE_PREVIEW（ST-S12-02）；
- universe 行 + committed 回执 + UniverseSaved 审计单事务；幂等重放返回原 universe_id；
- 导入按表头识别行情/主档/财务混合文件；声明 auxiliary_kind 时强制单类；
- 修复批次 1 遗留：取消标记仅终态回收（Started/Progress 不再打断取消通路）、JSONL 单缓冲原子写入、Decimal128 scale 对齐。

## 批次 3a：S13 信号内核与提交校验（2026-10-01）

**覆盖用例**：UT-S13-01、UT-S13-06、UT-S13-14、UT-S13-15（4/4 pass；全量回归 18/18）

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| 指标 | src/indicators.rs | 十进制 EMA（首值播种）与动量（skip 位移不用未来数据）；预热不足返回 None |
| 选股 | src/signals.rs | select_top_k：正分占槽、平分按代码升序、不足 K 留现金不加权 |
| 运行契约 | src/protocol.rs | StrategySpec/CostSpec/RunSpec 与提交前校验（样本外>训练结束、网格≤100、费率区间、confirmed 强制、生效区间覆盖） |

**未覆盖（批次 3b）**：UT-S13-02~05/07~13 与 ST-S13-01~03——真实 Nautilus 引擎编排、
隔日执行适配层、费用/拒单/公司行为、运行任务生命周期与网格子任务。
已验证 `cargo check -p nautilus-backtest --offline` 可行（33s），引擎集成无依赖阻塞。

## 待办批次（未实现，不代表可用）

- 批次 3b：S13 运行编排（真实 Nautilus 引擎、隔日执行适配层、UT-S13-02~05/07~13、ST-S13-01~03）
- 批次 4：S14 比较（CompareRuns/QueryRows 扩展 equity/holdings/fills）
- 批次 5：S15 交易计划（GeneratePlan/ExportPlan/SaveManualNote、UT-S15-05/06 性能量测）
- 批次 6+：S17～S20 工程台、egui GUI（依赖当前未缓存，需网络或 vendor）、smoke 双平台

> verify 口径说明：`openlogos verify` 的覆盖度门（Gate 3.5/3.6）统计全仓 101 条用例，
> 含引擎基线（ST-01~08、UT-AST/BT/EXEC 等，接入采用时未接 reporter）与未实现批次。
> 本提案范围内用例将随批次 3~5 收敛；基线用例的覆盖率口径需另行决策。
