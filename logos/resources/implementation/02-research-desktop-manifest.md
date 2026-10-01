# 研究桌面实现清单（astock-research-desktop）

> 状态：批次 6 完成（S11～S15 全量 + UT-S13-07 子进程补验；回归 42 pass + 4 skip，0 失败；剩余用例环境阻塞，见批次 6 清单）· 真相源：crates/research-domain、crates/research-worker、crates/research-testkit
> 上游规格：core-05-research-architecture.md / core-research-contracts.yaml / core-01-research-storage.sql / core-S11～S15-test-cases.md

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

**已验证 `cargo check -p nautilus-backtest --offline` 可行（33s），引擎集成无依赖阻塞。**

## 批次 3b：S13 引擎运行编排（2026-10-01）

**覆盖用例**：UT-S13-02、UT-S13-03、UT-S13-04、UT-S13-05、UT-S13-09、UT-S13-13、ST-S13-01
（7/7 pass；全量回归 25/25，JSONL 0 失败）

**交付物**（新 crate `crates/research-worker`）：

| 组件 | 路径 | 职责 |
|---|---|---|
| 费用模型 | src/fees.rs | astock_fee 纯函数（佣金 max(费率×额, 最低) 双向、印花税仅卖出）+ AStockFeeModel 接入引擎（FeeModelHandle 构造后替换） |
| 规则门 | src/gate.rs | 停牌/缺价/涨跌停/T+1/最小量/步长 拒单枚举含理由；size_buy 含费缩量、现金永不负 |
| 运行编排 | src/runner.rs | run_ema_daily：BacktestEngine 装配（Cash/CNY/L1_MBP），日线 bar（07:00 UTC）+ 开盘集合竞价 trade tick（06:00 UTC）双数据流；规范化 RunOutcome（无 UUID/墙钟） |

**隔日执行机制（实测校正）**：引擎市价单在提交瞬间按当前市价同步撮合，
`on_bar(T 收盘)` 直接下单会以 T 收盘成交（不符合隔日语义）。改为：收盘 bar 只记录信号，
次日开盘价合成集合竞价 trade tick 到达时（先于当日 bar）提交市价单 → 以 T+1 开盘价成交。
ST-S13-01 数值断言：信号日 1/3 收 11 不成交，1/4 开盘 12 成交 100 股，现金 800、净值 2100、收益 0.05。

**踩坑记录**：bar 价格字符串须带标的价格精度（"10.00" 非 "10"），否则撮合引擎以精度不符跳过全部 bar
（报 No market 拒单）；Money Display 含币种后缀，佣金取 `Money::as_decimal()` 不经字符串解析。

**未覆盖（批次 3c）**：UT-S13-07（5s 杀子进程）、UT-S13-08/11（公司行为）、UT-S13-10（日历调仓）、
UT-S13-12（规则包估值）、ST-S13-02（取消/崩溃）、ST-S13-03（网格父子任务）——
SubmitRun 协调器编排与子进程隔离，依赖本批 runner 作为计算内核。

## 批次 3c：S13 运行生命周期（2026-10-01）

**覆盖用例**：UT-S13-07、ST-S13-02、ST-S13-03（3/3 pass；全量回归 28/28，JSONL 0 失败）

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| 运行编排 | research-domain/src/coordinator.rs | SubmitRun（存在性预检→幂等入队→网格父子拆分→专属守望线程）；RunExited 产物裁决；网格父任务聚合 |
| 执行器挂载 | research-domain/src/executor.rs | 进程内执行器注册点（领域核心不反向依赖引擎 crate）；退出码契约 0/3/4 |
| 引擎适配 | research-worker/src/adapter.rs | 读配置文档→装配快照行情→组合 EMA 运行→产物写对象存储→结果清单写 tmp/{task_id}/result.json |
| 子进程入口 | research-worker/src/bin/run_task.rs | `research-run-task <workspace> <config_hash>`；环境清空（凭证红线） |
| 存储扩展 | research-domain/src/store.rs | task.parent_id/retry_of 读写；research_run 表 API；request_id 子任务后缀避让 UNIQUE |
| 协议扩展 | research-domain/src/protocol.rs | TaskView.children/parent_id；RunCompleted/GridResolved 事件 |

**关键语义（对照规格）**：
- 完成提交与取消竞争：以持久化事务为准，完成先行则 CancelTask 返回 ALREADY_TERMINAL 不删结果（UT-S13-07）；
- 取消/崩溃：取消无已提交结果；重试同键返回原任务、新键新 task_id；重启扫描不改写旧终态（ST-S13-02）；
- 网格：子任务各自存档（产物哈希互异），取消只影响未完成项，父任务聚合 succeeded 需子运行清单产物
  （DDL 约束 succeeded 必须有 result_hash），不把整个网格标全部成功（ST-S13-03）；
- 子进程隔离预留：`CoordinatorConfig.worker_bin` 配置后走真实进程（env_clear 不继承 TICKFLOW_API_KEY）；
  未配置时走注册的进程内执行器（测试路径，同一裁决逻辑）。5 秒强杀与 UT-S13-07 后半在子进程接线时补验。

**踩坑记录**：ObjectStore::put 并发同哈希共用固定 `.tmp` 名产生 rename 竞态（两个网格子运行
结果恰好相同）——临时文件名加原子计数序号，rename 目标覆盖等价安全；SQLite 只读连接不能回填
运行结果，工作器以 tmp 清单文件传递结果哈希，元数据库仍只由协调器写（架构红线）。

**未覆盖（批次 3d）**：UT-S13-08/11（公司行为：拆股/分红事件流）、UT-S13-10（日历调仓：节假日周末）、
UT-S13-12（规则包估值：缺历史生效区间严格失败）——需要公司行为与规则包数据通道；
UT-S13-07 的 5 秒强杀计时在 worker_bin 子进程接线时补验。

## 批次 3d：S13 公司行为/日历调仓/规则包估值（2026-10-01）

**覆盖用例**：UT-S13-08、UT-S13-10、UT-S13-11、UT-S13-12（4/4 pass；全量回归 32/32，JSONL 0 失败）

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| 公司行为纯函数 | research-domain/src/corporate.rs | apply_action（拆股调整持仓，公告可见性守卫）；DividendLedger/advance_dividends（除息计应收、支付转现金）；rebalance_signal_dates/next_trade_date（ISO 周按儒略日分桶，周一为界）；resolve_rules/first_missing_rule_date（前闭后开生效区间） |
| 辅助数据通道 | research-domain/src/auxiliary.rs | actions/calendar/rules 三类暂存 CSV 逐行校验 + 规范化 JSON 字节 + 哈希校验读回（CorruptArtifact） |
| 规则门迁移 | research-domain/src/gate.rs | 自 research-worker 迁入（域层不反向依赖工作器）；worker 侧改再导出 |
| 导入分发 | research-domain/src/coordinator.rs | 五类 auxiliary 全量接入（master/financial/actions/calendar/rules） |
| 引擎接入 | research-worker/src/runner.rs | EmaRunConfig 增加 signal_dates（调仓门控：非信号日形成不了委托）与 actions；末段估值重放应用公司行为（净值=现金+持仓×收盘+应收股息） |
| 组合适配 | research-worker/src/adapter.rs | 严格模式规则包全区间预检（失败定位标的/板块/日期）；规则按首日解析；调仓信号日组合级一致 |

**关键语义（对照用例）**：
- UT-S13-08 拆股：除权日持仓 ×比例（100→200）、净值 800+200×6=2000 不凭空减半；
  公告晚于除权日的事件不可见、不应用（时点纪律）；
- UT-S13-11 分红：除息日应收 100×0.1=10 计入总资产一次（净值 2010），支付日应收
  转现金（现金 810）、净值仍 2010 不重复增加；
- UT-S13-10 日历调仓：weekly 信号日=每周最后交易日（1/12、1/19）、执行日为下一
  交易日（1/15、1/22，跨周末顺延）；monthly 只在月末最后交易日；日历不含的
  节假日周末天然不产生信号；
- UT-S13-12 规则包：前闭后开生效区间（切换日 6/30 起新规则）；严格模式缺生效
  区间任务失败并定位板块/日期（含「不得以现行规则反套历史」声明）。

**已知限制（诚实边界）**：
- 引擎 CASH 账户禁止卖空且无持仓调整公开接口：拆股不改变引擎内持仓，策略侧
  卖出按引擎持仓封顶（如拆股后引擎卖出 100 而非 200），溢余股数在估值层保留
  ——公司行为只在估值重放层生效；引擎内公司行为事件需引擎侧支持，后续批次；
- 规则包逐日版本化解析未做（按首日解析；严格预检保证全区间覆盖）；
- 分红现金不回补引擎账户（BacktestEngine 无公开账户调整入口），买入规模不含
  分红现金（保守方向，不会超买）；
- UT-S13-07 的 5 秒强杀计时仍在 worker_bin 子进程接线时补验。

**踩坑记录**：儒略日数 0 是周一，周分桶用 `days/7` 即可；原实现 `(days+3)/7`
使周界移到周五、周信号错误落在周四（UT-S13-10 首跑暴露）。引擎对「卖出量 >
净持仓」按 CASH 账户卖空拒绝——公司行为若在策略侧调整持仓会触发该拒绝，
这是「估值层生效」设计的直接原因。

## 批次 4：S14 比较（2026-10-01）

**覆盖用例**：UT-S14-01、UT-S14-02、UT-S14-03、UT-S14-04、ST-S14-01（5/5 pass；全量回归 37/37，JSONL 0 失败）

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| 指标纯函数 | research-domain/src/metrics.rs | compute_metrics（总收益/年化/回撤/波动/Sharpe/换手/成本）；NullableMetric（None+原因，十进制定点无 NaN）；十进制 Newton 平方根（确定性）；完整序列计算不降采样 |
| 产物契约形状 | research-domain/src/worker_api.rs | EquityDoc（trade_date/现金/持仓市值/应收/净值）、HoldingsDoc（数量/可卖/标记价/市值）、FillDoc（fill_id 规范化） |
| 持仓时间线 | research-worker/src/runner.rs | 估值重放产出逐日持仓（含 T+1 可卖数与应收股息） |
| 组合适配 | research-worker/src/adapter.rs | 成员估值按字段求和（现金/市值/应收/净值）；持仓拼接；fill_id F-标的-日期-序 |
| 比较命令 | research-domain/src/coordinator.rs | compare_runs（2..5 校验→指标→交集→差异→基准）；NO_OVERLAP；差异显式列出（区间/费用/数据/模式）不排名 |
| 基准指标 | 同上 | 等权买入持有（首日收盘建仓）；期望日期取运行净值序列∩窗口，任一成员缺日期→基准指标全空附原因，策略指标保留 |
| 查询扩展 | 同上 | QueryRows 运行三桶 equity/holdings/fills（RowsRow 类型化枚举，游标绑定结果哈希） |

**关键语义（对照用例）**：
- UT-S14-01：净值 100/110/99 → 总收益 -0.01、回撤 0.1；降采样会丢中间峰值（对照断言证明按完整序列）；
- UT-S14-02：零波动/单样本 → Sharpe 空+原因（「波动为零」「仅1个收益样本」），绝不出现 NaN；
- UT-S14-03：区间不相交交集请求 NO_OVERLAP（完整视图不拒绝、overlap 空）；
  基准缺日期只清空基准指标；
- UT-S14-04：结果对象文件篡改（旧哈希引用不变）→ 比较/查询均 CORRUPT_ARTIFACT
  （对象存储读时哈希校验，无缓存旁路）；
- ST-S14-01：费用差异显式列出（含双方费率与运行 ID）；各运行独立指标行；
  Comparison 结构无排名字段（编译期保证不静默排名）。

**修复记录（批次 3c 遗留）**：research_run.result_hash 此前从未回填（handle_run_exit
只更新任务表）——S14 比较经 research_run 读产物时暴露；已并入 commit_task_result
同一事务回填（先修过一次「提交后单独回填」仍有终态可见先于回填的竞态，测试探针
偶然通过暴露）。

**已知限制**：年化收益为首版线性近似（总收益×252/样本数，几何年化随指标版本化替换）；
RowsQuery 的 sort 参数未实现（与 S12 一致）；基准为首日收盘等权买入持有近似。

## 批次 5：S15 交易计划（2026-10-01）

**覆盖用例**：UT-S15-01、UT-S15-02、UT-S15-03、UT-S15-04、ST-S15-01（5/5 pass）；
UT-S15-05/06、ST-S15-02/03 以 skip 诚实上报（需参考机与 GUI，批次 6；skip 不可计为通过）。
全量回归 42 pass + 4 skip，JSONL 0 失败。

**交付物**：

| 组件 | 路径 | 职责 |
|---|---|---|
| 计划纯函数 | research-domain/src/plan.rs | plan_target（floor(总资产×权重/价/步长)×步长）、HoldingInput 校验（现金非负/0≤可卖≤持有/总资产为正，错误定位字段）、csv_escape（公式前缀 =+-@ 加撇号防注入）、check_stale_as_of（早于覆盖末日默认 STALE_DATA，显式确认转历史计划）、TradePlanDoc.csv_bytes（UTF-8 BOM；行级+全局限制、data_version、historical-plan 标识逐行落列） |
| 计划命令 | research-domain/src/coordinator.rs | GeneratePlan（幂等回执+快照/池存在性预检→trade_plan 任务）；计算线程（陈旧预检→pass 成员→as_of 收盘参考价→等权目标→限制说明→产物入库→PLAN_READY 审计）；GetTradePlan（kind 校验+终态检查）；ExportPlan（仅 csv_utf8_bom；已存在未确认→PATH_CONFLICT；临时文件+原子改名；回执含 sha256/行数）；SaveManualNote（类型白名单 备注/已人工处理/放弃；内容寻址 note_id，同内容同 ID，追加不改 fills） |
| 协议扩展 | research-domain/src/protocol.rs | PlanSpec（allow_historical 默认 false）、event PLAN_READY、QueryRows Plan 桶（RowsRow::Plan 类型化行） |

**关键语义（对照用例）**：
- UT-S15-01：10000×0.2/10=200 股、持 100 建议买 100、参考额 1000；纯函数无
  订单/委托字段——全 crate 不存在 broker/order 通道类型（FR-R09 编译期保证）；
- UT-S15-02：可卖 200 > 持有 100 拒绝并定位 positions[0].sellable_quantity；
  现金为负定位 cash_cny（提交时同步校验，不入队）；
- UT-S15-03：`=1+1` 导出为 `"'=1+1"`（引号包裹+撇号）；含中文与空格路径
  正常落盘；已存在未确认 → PATH_CONFLICT，确认后覆盖；
- UT-S15-04：as_of 早于快照覆盖末日默认任务失败 STALE_DATA（field=as_of）；
  allow_historical=true 成功且 doc.historical=true、CSV 逐行含 historical-plan；
- ST-S15-01：生成（参考价=as_of 收盘 33，目标 floor(20000/33/100)×100=600，
  差额 +500）→ 导出（回执 sha256 与磁盘文件一致、行数=计划行数）→ 备注
  （同内容同 note_id；未知类型拒绝）→ 回测 fills 前后完全一致。

**已知限制**：备注仅内容寻址入库（无 plan→note 索引表，列备注待 GUI 批次）；
目标股数固定 100 股步长（A股整手，未读规则包 qty_step）；
UT-S15-05/06 性能量测与 ST-S15-02/03 双平台 GUI 旅程留待批次 6。

## 批次 6：UT-S13-07 子进程补验 + 环境阻塞清算（2026-10-01）

**覆盖用例**：UT-S13-07 后半（worker_bin 子进程）补验通过（同一用例 ID，1.2s 实测）；
全量回归 42 pass + 4 skip，JSONL 0 失败。

**补验内容（s13_run.rs 竞争 C，真实子进程路径）**：
- 凭证红线端到端：测试进程注入探针 `TICKFLOW_API_KEY`，协调器以 env_clear 启动
  worker_bin 子进程，子进程转储环境断言不含变量名与探针值；
- 5 秒强杀上限：取消 → cancelled 终态实测 <5s（实现为标记置位后 50ms 轮询内
  SIGKILL，严于规格上限）；无已提交结果（artifact_hash 为空）；
- 专属进程死亡：kill -0 确认工作进程已被终止（协调器 kill 后 wait 收割）。

**S17～S20 原型编排检查（原型验收线，非生产代码）**：
- 统一离线 HTML 编排检查 13/13 pass（ST-S17-11~14、ST-S18-11~13、ST-S19-11~13、
  ST-S20-11~13，见 logos/changes/astock-research-desktop/prototype-review/
  unified-test-results.jsonl，source 标明非生产验收）；
- 业务逻辑补充 3/3 pass（ST-S17-15、ST-S18-14、ST-S19-14，business-test-results.jsonl）。

**环境阻塞清单（本机不可执行，诚实上报 skip 或待环境）**：
| 用例 | 阻塞原因 | 状态 |
|---|---|---|
| UT-S15-05/06 | 需 Windows/Linux 参考机与生产 GUI（egui/eframe 不在 cargo 离线缓存，需网络或 vendor） | skip 上报 |
| ST-S15-02/03 | 需生产 GUI 双平台旅程（同上） | skip 上报 |
| smoke 双平台 | 需按 core-03-desktop-delivery.md 部署后执行（部署与 smoke 均为人类确认点） | 待部署 |

**CPU 红线（验收主红线）落实汇总**：协调线程 crossbeam select! 阻塞无轮询空转；
计算线程随任务生灭；取消 50ms 粒度响应；wait_terminal 固定 50ms 睡眠；
子进程守望 50ms 轮询（本批实测取消终态 1.2s << 5s 上限）。

## 待办批次（环境阻塞，非本机可解）

- egui 生产 GUI（依赖需网络下载或 vendor 后构建）→ 解锁 UT-S15-05/06、ST-S15-02/03；
- 双平台部署 + smoke（人类确认点）；UT-S15-05/06 性能量测需参考机环境记录。
- verify 覆盖度门（Gate 3.5/3.6）统计全仓 101 条用例，含引擎基线用例（接入采用时
  未接 reporter）——基线用例的覆盖率口径需另行决策。

> verify 口径说明：`openlogos verify` 的覆盖度门（Gate 3.5/3.6）统计全仓 101 条用例，
> 含引擎基线（ST-01~08、UT-AST/BT/EXEC 等，接入采用时未接 reporter）与未实现批次。
> 本提案范围内用例将随批次 3~5 收敛；基线用例的覆盖率口径需另行决策。
