# 研究桌面实现清单（astock-research-desktop）

> 状态：批次 1 完成（S11）· 真相源：crates/research-domain、crates/research-testkit
> 上游规格：core-05-research-architecture.md / core-research-contracts.yaml / core-01-research-storage.sql / core-S11-test-cases.md

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

## 待办批次（未实现，不代表可用）

- 批次 2：S12 股票池（PreviewUniverse/SaveUniverse、RuleAST、严格/探索模式）
- 批次 3：S13 研究运行（SubmitRun、工作进程 JSONL、引擎隔日执行适配）
- 批次 4：S14 比较（CompareRuns/QueryRows）
- 批次 5：S15 交易计划（GeneratePlan/ExportPlan/SaveManualNote、UT-S15-05/06 性能量测）
- 批次 6+：S17～S20 工程台、egui GUI（依赖当前未缓存，需网络或 vendor）、smoke 双平台
