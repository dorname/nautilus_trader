//! 引擎执行适配：把研究运行配置装配为 Nautilus 引擎运行并提交产物。
//!
//! `register_in_process()` 由测试/进程内模式调用；`run_task()` 供工作进程
//! 二进制（`run-task <workspace> <config_hash>`）使用——两者共享同一裁决逻辑。
//! 退出码契约见 research-domain::executor。

use std::{collections::BTreeMap, path::Path, sync::atomic::Ordering};

use rust_decimal::Decimal;

use nautilus_research_domain::{
    auxiliary::{ActionRecord, CalendarRecord, RuleRecord},
    coordinator::{Completion, RunConfigDoc},
    corporate,
    error::{ErrorCode, ResearchError},
    executor,
    manifest::SnapshotManifest,
    objects::ObjectStore,
    parquet_io,
    protocol::{Rebalance, RunSpec, UniverseMode},
    store::MetadataStore,
    worker_api::{EquityDoc, FillDoc, HoldingsDoc, RunOutcomeDoc},
};

use crate::{
    gate::BoardRules,
    runner::{run_ema_daily, DayBar, EmaRunConfig},
};

/// 注册进程内执行器（引擎与协调器同进程，测试路径）。
pub fn register_in_process() {
    executor::register_run_executor(std::sync::Arc::new(|workspace, config_hash, cancel, task_id, done| {
        match execute(workspace, config_hash, cancel, task_id, done) {
            Ok(code) => code,
            Err(e) => {
                let _ = done.send(Completion::Failed {
                    task_id: task_id.to_string(),
                    error: e,
                });
                1
            }
        }
    }));
}

/// 工作进程入口：执行一次运行并返回进程退出码。
pub fn run_task(workspace: &Path, config_hash: &str) -> i32 {
    let (done_tx, done_rx) = crossbeam::channel::bounded::<Completion>(64);
    // 工作进程内进度直接丢弃（stdout 是协议通道，不写日志；stderr 才是日志）
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let probe = || cancel.load(Ordering::SeqCst);
    let result = execute(workspace, config_hash, &probe, "worker", &done_tx);
    // 消费完成通知避免阻塞
    for c in done_rx.try_iter() {
        if let Completion::Failed { error, .. } = c {
            eprintln!("run-task 失败：{:?}", error.code);
            return 1;
        }
    }
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("run-task 失败：{:?}", e.code);
            1
        }
    }
}

/// 共享执行路径：读配置 → 装配 → 引擎 → 写产物 → 回填哈希。返回退出码。
fn execute(
    workspace: &Path,
    config_hash: &str,
    cancel: &dyn Fn() -> bool,
    task_id: &str,
    done: &crossbeam::channel::Sender<Completion>,
) -> Result<i32, ResearchError> {
    let send_progress = |stage: &str, d: u64, total: u64| {
        let _ = done.send(Completion::Progress {
            task_id: task_id.to_string(),
            stage: stage.to_string(),
            done: d,
            total,
        });
    };
    let store = MetadataStore::open_readonly(&workspace.join("research.db"))?;
    let run = store
        .get_research_run(task_id)?
        .ok_or_else(|| ResearchError::not_found(format!("研究运行不存在：{task_id}")))?;
    if run.config_hash != config_hash {
        return Err(ResearchError::new(
            ErrorCode::CorruptArtifact,
            format!("运行配置哈希不符：{} ≠ {}", run.config_hash, config_hash),
        ));
    }
    let doc: RunConfigDoc = serde_json::from_str(&run.config_json)
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("运行配置解析失败：{e}")))?;

    let objects = ObjectStore::new(workspace)?;
    let snapshot = store
        .get_snapshot(&doc.snapshot_id)?
        .ok_or_else(|| ResearchError::not_found(format!("快照不存在：{}", doc.snapshot_id)))?;
    let manifest: SnapshotManifest = serde_json::from_slice(&objects.get(&snapshot.manifest_hash)?)
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("快照清单解析失败：{e}")))?;

    // 成员：已保存股票池的成员产物
    let universe = store
        .get_universe(&doc.universe_id)?
        .ok_or_else(|| ResearchError::not_found(format!("股票池不存在：{}", doc.universe_id)))?;
    let members_doc: PreviewDocLike = serde_json::from_slice(&objects.get(&universe.members_hash)?)
        .map_err(|e| ResearchError::new(ErrorCode::CorruptArtifact, format!("成员产物解析失败：{e}")))?;
    let members: Vec<String> = members_doc.pass_members();
    if members.is_empty() {
        return Err(ResearchError::new(
            ErrorCode::EmptyUniverse,
            format!("股票池 {} 无 pass 成员，无法运行", doc.universe_id),
        ));
    }

    send_progress("装配行情", 0, 1);
    let mut quotes_by_symbol: BTreeMap<String, Vec<nautilus_research_domain::quotes::QuoteRow>> = BTreeMap::new();
    for part in &manifest.partitions {
        if cancel() {
            return Ok(executor::EXIT_CANCELLED);
        }
        let path = objects.path_of(&part.sha256);
        for row in parquet_io::read_quotes_partition(&path)? {
            quotes_by_symbol.entry(row.instrument_id.clone()).or_default().push(row);
        }
    }
    let mut actions = Vec::new();
    let mut calendar = Vec::new();
    let mut rules = Vec::new();
    for aux in &manifest.auxiliary {
        let bytes = objects.get(&aux.sha256)?;
        match aux.kind.as_str() {
            "actions" => actions = nautilus_research_domain::auxiliary::actions_from_bytes(&bytes)?,
            "calendar" => calendar = nautilus_research_domain::auxiliary::calendar_from_bytes(&bytes)?,
            "rules" => rules = nautilus_research_domain::auxiliary::rules_from_bytes(&bytes)?,
            _ => {}
        }
    }

    // 网格覆盖参数
    let mut spec = doc.spec.clone();
    apply_grid_override(&mut spec, &doc.grid_override)?;

    send_progress("引擎运行", 0, 1);
    let outcome = run_portfolio(
        &spec,
        &members,
        &quotes_by_symbol,
        &actions,
        &calendar,
        &rules,
        cancel,
    )?;

    if cancel() {
        return Ok(executor::EXIT_CANCELLED);
    }

    // 产物：规范化结果文档（无 run_id/墙钟），写对象存储；
    // 结果哈希写工作器私有清单文件（元数据库只由协调器写入）
    let bytes = nautilus_research_domain::hash::canonical_json(&outcome).into_bytes();
    let result_hash = objects.put(&bytes)?;
    let tmp = workspace.join("tmp").join(task_id);
    std::fs::create_dir_all(&tmp).map_err(|e| {
        ResearchError::invalid(format!("创建结果清单目录失败：{e}"))
    })?;
    std::fs::write(tmp.join("result.json"), result_hash.as_bytes()).map_err(|e| {
        ResearchError::invalid(format!("写结果清单失败：{e}"))
    })?;
    Ok(executor::EXIT_OK)
}

/// 网格覆盖（fast/slow/lookback/skip/top_k）。
fn apply_grid_override(spec: &mut RunSpec, grid: &BTreeMap<String, String>) -> Result<(), ResearchError> {
    for (name, value) in grid {
        let v = value
            .parse::<u32>()
            .map_err(|_| ResearchError::invalid(format!("网格参数 {name} 值非法：{value}")))?;
        match name.as_str() {
            "fast" => spec.strategy.fast = Some(v),
            "slow" => spec.strategy.slow = Some(v),
            "lookback" => spec.strategy.lookback = Some(v),
            "skip" => spec.strategy.skip = Some(v),
            "top_k" => spec.strategy.top_k = v,
            _ => {}
        }
    }
    Ok(())
}

/// 成员产物最小读取面（避免依赖 coordinator 的 PreviewDoc 完整结构）。
#[derive(serde::Deserialize)]
struct PreviewDocLike {
    rows: Vec<MemberRowLike>,
}

#[derive(serde::Deserialize)]
struct MemberRowLike {
    instrument_id: String,
    verdict: String,
}

impl PreviewDocLike {
    fn pass_members(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter(|r| r.verdict == "pass")
            .map(|r| r.instrument_id.clone())
            .collect()
    }
}

/// 首版板块推断：合成标的均视为主板（真实板块划分随主档数据通道扩展）。
fn default_board(_code: &str) -> String {
    "主板".to_string()
}

/// 组合运行：等权多标的 EMA（首版：每标的独立 EMA 信号 + 等槽位资金）。
/// F-RUN 覆盖单标的情形；多标的按代码升序分槽。
/// 规则包按板块生效区间解析——严格模式缺生效区间即任务失败并定位标的/日期
/// （UT-S13-12）；交易日历驱动调仓（weekly/monthly 只在周期最后交易日形成
/// 信号，UT-S13-10）；公司行为按除权除息日调整持仓与股息应收（UT-S13-08/11）。
#[allow(clippy::too_many_arguments)]
fn run_portfolio(
    spec: &RunSpec,
    members: &[String],
    quotes: &BTreeMap<String, Vec<nautilus_research_domain::quotes::QuoteRow>>,
    actions: &[ActionRecord],
    calendar: &[CalendarRecord],
    rules_pack: &[RuleRecord],
    cancel: &dyn Fn() -> bool,
) -> Result<RunOutcomeDoc, ResearchError> {
    let capital = Decimal::from_str_exact(&spec.capital_cny)
        .map_err(|_| ResearchError::invalid("capital_cny 非十进制"))?;
    // 严格模式：规则包存在时运行区间每一天都必须有生效规则（先于运行定位缺失日）
    if !rules_pack.is_empty() && spec.mode == UniverseMode::Strict {
        for code in members {
            let Some(rows) = quotes.get(code) else {
                continue;
            };
            let board = default_board(code);
            let missing = rows
                .iter()
                .map(|r| r.trade_date.as_str())
                .filter(|d| *d >= spec.start.as_str() && *d <= spec.end.as_str())
                .find(|d| corporate::resolve_rules(rules_pack, &board, d).is_err());
            if let Some(date) = missing {
                return Err(ResearchError::new(
                    ErrorCode::MissingCapability,
                    format!(
                        "严格任务失败：标的 {code}（板块 {board}）在 {date} 无生效规则，不得以现行规则反套历史或默认值代替"
                    ),
                ));
            }
        }
    }
    // 调仓信号日（组合级一致；节假日周末不在日历中，周期尾自动落在最后交易日）
    let signal_dates: Option<std::collections::BTreeSet<String>> =
        if calendar.is_empty() || spec.strategy.rebalance == Rebalance::Daily {
            None
        } else {
            Some(
                corporate::rebalance_signal_dates(
                    calendar,
                    spec.strategy.rebalance,
                    &spec.start,
                    &spec.end,
                )
                .into_iter()
                .collect(),
            )
        };

    let slots = members.len().max(1);
    let slot_capital = capital / Decimal::from(slots as u64);
    let mut fills_doc: Vec<FillDoc> = Vec::new();
    let mut holdings_doc: Vec<HoldingsDoc> = Vec::new();
    // 组合净值：各成员按字段求和（现金/持仓市值/应收/净值）
    let mut combined: BTreeMap<String, [Decimal; 4]> = BTreeMap::new();
    let mut final_equity = Decimal::ZERO;
    for (idx, code) in members.iter().enumerate() {
        if cancel() {
            return Ok(RunOutcomeDoc::default());
        }
        let bars: Vec<DayBar> = quotes
            .get(code)
            .map(|rows| {
                rows.iter()
                    .map(|r| DayBar {
                        date: r.trade_date.clone(),
                        open: r.open,
                        high: r.high,
                        low: r.low,
                        close: r.close,
                        volume: r.volume_shares,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if bars.is_empty() {
            continue;
        }
        // 规则按首日解析（严格模式已预检全区间覆盖；逐日版本化解析后续扩展）；
        // 无规则包数据时用合成规则（探索口径）
        let rules = if rules_pack.is_empty() {
            BoardRules {
                board: "synthetic".to_string(),
                tick: "0.01".parse().unwrap(),
                min_qty: 100,
                qty_step: 100,
                limit_pct: None,
            }
        } else {
            corporate::resolve_rules(rules_pack, &default_board(code), &bars[0].date)?
        };
        // 该标的公司行为流（除权除息）
        let code_actions: Vec<ActionRecord> = actions
            .iter()
            .filter(|a| a.instrument_id == *code)
            .cloned()
            .collect();
        let config = EmaRunConfig {
            fast: spec.strategy.fast.unwrap_or(1),
            slow: spec.strategy.slow.unwrap_or(2),
            warmup_bars: spec.strategy.lookback.unwrap_or(20) as usize,
            capital: slot_capital,
            commission_rate: Decimal::from_str_exact(&spec.costs.commission_rate).unwrap_or(Decimal::ZERO),
            min_commission: Decimal::from_str_exact(&spec.costs.min_commission_cny).unwrap_or(Decimal::ZERO),
            sell_tax_rate: Decimal::from_str_exact(&spec.costs.sell_tax_rate).unwrap_or(Decimal::ZERO),
            other_fee_rate: Decimal::from_str_exact(&spec.costs.other_fee_rate).unwrap_or(Decimal::ZERO),
            rules: rules.clone(),
            signal_dates: signal_dates.clone(),
            actions: code_actions,
        };
        let outcome = run_ema_daily(code, "SSE", &bars, &config)
            .map_err(|e| ResearchError::new(ErrorCode::RunNotReady, format!("引擎运行失败：{e}")))?;
        for point in &outcome.equity_curve {
            let entry = combined.entry(point.date.clone()).or_default();
            entry[0] += Decimal::from_str_exact(&point.cash_cny).unwrap_or(Decimal::ZERO);
            let qty = Decimal::from(point.position_qty);
            let close = Decimal::from_str_exact(&point.close).unwrap_or(Decimal::ZERO);
            entry[1] += qty * close;
            entry[2] += Decimal::from_str_exact(&point.receivable_cny).unwrap_or(Decimal::ZERO);
            entry[3] += Decimal::from_str_exact(&point.equity_cny).unwrap_or(Decimal::ZERO);
        }
        for h in &outcome.holdings {
            holdings_doc.push(HoldingsDoc {
                trade_date: h.date.clone(),
                instrument_id: code.clone(),
                quantity: h.quantity,
                sellable_quantity: h.sellable_quantity,
                mark_price: h.mark_price.clone(),
                market_value_cny: h.market_value_cny.clone(),
            });
        }
        // 每标的日内成交序号（fill_id 规范化且唯一）
        let mut seq_by_date: BTreeMap<String, u64> = BTreeMap::new();
        for fill in &outcome.fills {
            let seq = seq_by_date.entry(fill.date.clone()).or_insert(0);
            *seq += 1;
            fills_doc.push(FillDoc {
                fill_id: format!("F-{code}-{}-{:03}", fill.date, seq),
                instrument_id: code.clone(),
                trade_date: fill.date.clone(),
                side: fill.side.clone(),
                quantity: fill.qty,
                raw_price: fill.price.clone(),
                fees_cny: fill.fee_cny.clone(),
                reason: String::new(),
            });
        }
        final_equity += Decimal::from_str_exact(&outcome.final_equity_cny).unwrap_or(Decimal::ZERO);
        let _ = idx;
    }
    let total_return = if capital.is_zero() {
        Decimal::ZERO
    } else {
        (final_equity / capital) - Decimal::ONE
    };
    Ok(RunOutcomeDoc {
        kind: "research_run".to_string(),
        snapshot_id: spec.snapshot_id.clone(),
        universe_id: spec.universe_id.clone(),
        mode: spec.mode.as_str().to_string(),
        strategy: format!("{:?}", spec.strategy.template),
        fills: fills_doc,
        equity_curve: combined
            .into_iter()
            .map(|(trade_date, e)| EquityDoc {
                trade_date,
                cash_cny: e[0].normalize().to_string(),
                positions_value_cny: e[1].normalize().to_string(),
                receivables_cny: e[2].normalize().to_string(),
                equity_cny: e[3].normalize().to_string(),
            })
            .collect(),
        holdings: holdings_doc,
        final_equity_cny: final_equity.normalize().to_string(),
        total_return: total_return.normalize().to_string(),
    })
}


