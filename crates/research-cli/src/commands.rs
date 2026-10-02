//! 子命令实现：构造类型化 spec → 协调器命令 → 双形态输出。
//!
//! 薄壳纪律：业务校验与语义全部由协调器承载（ImportSpec::validate 等），
//! 本层只做参数→类型的机械转换与幂等键派生（`--idempotency-key` 显式键优先，
//! 缺省由规范化请求哈希派生——同参数重跑返回原任务，即幂等回执语义）。

use std::collections::BTreeMap;
use std::io::Write;
use std::time::Duration;

use nautilus_research_domain::hash::hash_canonical;
use nautilus_research_domain::plan::{ExportReceipt, ExportSpec, HoldingInput, ManualNote, NoteRef};
use nautilus_research_domain::protocol::{
    CompareSpec, CompareView, CostSpec, EffectiveRange, ImportSource, ImportSpec,
    Membership, MissingPolicy, PlanSpec, PriceBasis, Rebalance, RunSpec, SaveUniverseSpec,
    SnapshotPage, StrategySpec, StrategyTemplate, TaskRef, TaskView, UniverseMode, UniversePreview,
    UniverseRef, UniverseSpec,
};
use nautilus_research_domain::universe::RuleGroup;
use nautilus_research_domain::{Coordinator, CoordinatorConfig, ResearchError};

use crate::args::{CostArgs, Global, Invocation, StrategyArgs};
use crate::output;

type CmdResult<T> = Result<T, ResearchError>;

/// 打开工作区协调器。未配置 worker_bin 时注册进程内执行器（同一裁决路径），
/// 便于本机直接运行；生产/隔离部署建议以子进程 worker_bin 运行。
fn open(g: &Global) -> CmdResult<Coordinator> {
    nautilus_research_worker::adapter::register_in_process();
    Coordinator::open(CoordinatorConfig::new(g.workspace.clone()))
}

/// 幂等键：显式键优先，否则取规范化请求哈希前 24 位十六进制。
fn derive_key<T: serde::Serialize>(spec: &T, explicit: &Option<String>) -> String {
    match explicit {
        Some(k) => k.clone(),
        None => format!("cli-{}", &hash_canonical(spec)[..24]),
    }
}

fn wait_view(
    c: &Coordinator,
    task: &str,
    wait: bool,
    timeout: u64,
) -> CmdResult<Option<TaskView>> {
    if wait {
        Ok(Some(c.wait_terminal(task, Duration::from_secs(timeout))?))
    } else {
        Ok(None)
    }
}

fn print_task(
    out: &mut impl Write,
    g: &Global,
    r: &TaskRef,
    v: Option<TaskView>,
) {
    if g.json {
        match &v {
            Some(view) => output::emit_json(out, view),
            None => output::emit_json(out, r),
        }
    } else {
        let rows = match &v {
            Some(view) => output::task_view_rows(view),
            None => output::task_ref_rows(r),
        };
        output::emit_kv(out, &rows)
    }
}

fn price_basis(s: &str) -> PriceBasis {
    match s {
        "qfq" => PriceBasis::Qfq,
        "hfq" => PriceBasis::Hfq,
        _ => PriceBasis::Raw,
    }
}

fn mode(s: &str) -> UniverseMode {
    if s == "exploratory" {
        UniverseMode::Exploratory
    } else {
        UniverseMode::Strict
    }
}

fn membership(s: &str) -> Membership {
    if s == "dynamic" {
        Membership::Dynamic
    } else {
        Membership::Fixed
    }
}

fn template(s: &str) -> StrategyTemplate {
    if s == "momentum" {
        StrategyTemplate::Momentum
    } else {
        StrategyTemplate::Ema
    }
}

fn rebalance(s: &str) -> Rebalance {
    match s {
        "weekly" => Rebalance::Weekly,
        "monthly" => Rebalance::Monthly,
        _ => Rebalance::Daily,
    }
}

fn strategy_spec(a: &StrategyArgs) -> StrategySpec {
    StrategySpec {
        template: template(&a.template),
        version: a.version.clone(),
        fast: a.fast,
        slow: a.slow,
        lookback: a.lookback,
        skip: a.skip,
        top_k: a.top_k,
        rebalance: rebalance(&a.rebalance),
        weight: "equal_slots".to_string(),
        signal_basis: "point_in_time_adjusted".to_string(),
    }
}

fn cost_spec(a: &CostArgs) -> CostSpec {
    CostSpec {
        commission_rate: a.commission_rate.clone(),
        min_commission_cny: a.min_commission.clone(),
        sell_tax_rate: a.sell_tax.clone(),
        other_fee_rate: a.other_fee.clone(),
        slippage_bps: a.slippage_bps.clone(),
        participation_rate: a.participation.clone(),
        effective_schedule: vec![EffectiveRange {
            start: a.eff_start.clone(),
            end: a.eff_end.clone(),
        }],
        confirmed: true,
    }
}

/// `fast=5,10;slow=20,30` → 网格参数映射（校验由 RunSpec::validate 承载）。
fn parse_grid(s: &str) -> CmdResult<Option<BTreeMap<String, Vec<String>>>> {
    if s.trim().is_empty() {
        return Ok(None);
    }
    let mut grid = BTreeMap::new();
    for part in s.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, values) = part
            .split_once('=')
            .ok_or_else(|| {
                ResearchError::invalid(format!("网格参数格式须为 名称=值,值：{part}"))
                    .with_field("grid")
            })?;
        let items: Vec<String> = values
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
            .collect();
        if items.is_empty() {
            return Err(ResearchError::invalid(format!("网格参数 {name} 枚举为空"))
                .with_field("grid"));
        }
        grid.insert(name.trim().to_string(), items);
    }
    Ok(if grid.is_empty() { None } else { Some(grid) })
}

fn parse_rule(text: &str) -> CmdResult<RuleGroup> {
    serde_json::from_str(text)
        .map_err(|e| ResearchError::invalid(format!("规则 JSON 解析失败：{e}")).with_field("rule"))
}

fn parse_holdings(text: &str) -> CmdResult<Option<HoldingInput>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(text).map(Some).map_err(|e| {
        ResearchError::invalid(format!("手工持仓 JSON 解析失败：{e}")).with_field("holdings")
    })
}

fn import_source(s: &str) -> ImportSource {
    match s {
        "tickflow" => ImportSource::Tickflow,
        "auxiliary" => ImportSource::Auxiliary,
        _ => ImportSource::Tdx,
    }
}

/// 分发入口：返回进程退出码（调用方负责 std::process::exit）。
pub fn dispatch(inv: Invocation, out: &mut impl Write) -> CmdResult<i32> {
    use crate::args::EXIT_OK;
    match inv {
        Invocation::Import {
            g,
            source,
            paths,
            symbols,
            start,
            end,
            price_basis: pb,
            auxiliary_kind,
            wait,
            wait_timeout,
            idem,
        } => {
            let spec = ImportSpec {
                source: import_source(&source),
                paths,
                symbols,
                start,
                end,
                price_basis: price_basis(&pb),
                auxiliary_kind,
            };
            let key = derive_key(&spec, &idem);
            let c = open(&g)?;
            let r = c.import_data(&format!("req-{key}"), &key, spec)?;
            let v = wait_view(&c, &r.task_id, wait, wait_timeout)?;
            print_task(out, &g, &r, v);
            Ok(EXIT_OK)
        }
        Invocation::UniversePreview {
            g,
            snapshot,
            as_of,
            mode: m,
            membership: mem,
            rule,
            missing_policy,
            wait,
            wait_timeout,
            idem,
        } => {
            let spec = UniverseSpec {
                snapshot_id: snapshot,
                as_of,
                mode: mode(&m),
                membership: membership(&mem),
                rule: parse_rule(&rule)?,
                missing_policy: missing_policy
                    .as_deref()
                    .map(|p| match p {
                        "ignore_condition" => MissingPolicy::IgnoreCondition,
                        _ => MissingPolicy::Exclude,
                    }),
            };
            let key = derive_key(&spec, &idem);
            let c = open(&g)?;
            let r = c.preview_universe(&format!("req-{key}"), &key, spec)?;
            let v = wait_view(&c, &r.task_id, wait, wait_timeout)?;
            if g.json {
                match &v {
                    Some(view) => {
                        let p: UniversePreview = c.universe_preview(&r.task_id)?;
                        let payload = serde_json::json!({"task": view, "preview": p});
                        output::emit_json(out, &payload);
                    }
                    None => output::emit_json(out, &r),
                }
            } else {
                match &v {
                    Some(view) if view.state.is_terminal() => {
                        if view.state == nautilus_research_domain::TaskState::Succeeded {
                            let p = c.universe_preview(&r.task_id)?;
                            output::emit_kv(out, &output::preview_rows(&p));
                        } else {
                            output::emit_kv(out, &output::task_view_rows(view));
                        }
                    }
                    _ => output::emit_kv(out, &output::task_ref_rows(&r)),
                }
            }
            Ok(EXIT_OK)
        }
        Invocation::UniverseSave {
            g,
            preview_task,
            preview_hash,
            input_hash,
            name,
        } => {
            let spec = SaveUniverseSpec {
                preview_task_id: preview_task,
                preview_hash,
                input_hash,
                name,
            };
            let key = derive_key(&spec, &None);
            let c = open(&g)?;
            let u: UniverseRef = c.save_universe(&format!("req-{key}"), &key, spec)?;
            if g.json {
                output::emit_json(out, &u);
            } else {
                output::emit_kv(out, &output::universe_rows(&u));
            }
            Ok(EXIT_OK)
        }
        Invocation::RunSubmit {
            g,
            snapshot,
            universe,
            strategy,
            start,
            end,
            training_end,
            test_start,
            capital,
            costs,
            rules_hash,
            mode: m,
            grid,
            seed,
            benchmark,
            wait,
            wait_timeout,
            idem,
        } => {
            let spec = RunSpec {
                snapshot_id: snapshot,
                universe_id: universe,
                strategy: strategy_spec(&strategy),
                start,
                end,
                training_end,
                test_start,
                capital_cny: capital,
                costs: cost_spec(&costs),
                rules_hash,
                benchmark_snapshot_id: benchmark,
                mode: mode(&m),
                grid: match &grid {
                    Some(s) => parse_grid(s)?,
                    None => None,
                },
                seed,
            };
            let key = derive_key(&spec, &idem);
            let c = open(&g)?;
            let r = c.submit_run(&format!("req-{key}"), &key, spec)?;
            let v = wait_view(&c, &r.task_id, wait, wait_timeout)?;
            print_task(out, &g, &r, v);
            Ok(EXIT_OK)
        }
        Invocation::RunCancel { g, task } => {
            let c = open(&g)?;
            let v = c.cancel_task(&task)?;
            if g.json {
                output::emit_json(out, &v);
            } else {
                output::emit_kv(out, &output::task_view_rows(&v));
            }
            Ok(EXIT_OK)
        }
        Invocation::Compare {
            g,
            runs,
            view,
            benchmark,
        } => {
            let spec = CompareSpec {
                run_ids: runs,
                view: if view == "intersection" {
                    CompareView::Intersection
                } else {
                    CompareView::Full
                },
                benchmark_snapshot_id: benchmark,
            };
            let c = open(&g)?;
            let cmp = c.compare_runs(&spec)?;
            if g.json {
                output::emit_json(out, &cmp);
            } else {
                output::emit_kv(out, &output::comparison_rows(&cmp));
            }
            Ok(EXIT_OK)
        }
        Invocation::PlanGenerate {
            g,
            snapshot,
            universe,
            as_of,
            strategy,
            costs,
            rules_hash,
            mode: m,
            allow_historical,
            holdings,
            wait,
            wait_timeout,
            idem,
        } => {
            let spec = PlanSpec {
                strategy: strategy_spec(&strategy),
                snapshot_id: snapshot,
                universe_id: universe,
                as_of,
                holding_version_id: None,
                holdings: match &holdings {
                    Some(text) => parse_holdings(text)?,
                    None => None,
                },
                costs: cost_spec(&costs),
                rules_hash,
                allow_historical,
                mode: mode(&m),
            };
            let key = derive_key(&spec, &idem);
            let c = open(&g)?;
            let r = c.generate_plan(&format!("req-{key}"), &key, spec)?;
            let v = wait_view(&c, &r.task_id, wait, wait_timeout)?;
            print_task(out, &g, &r, v);
            Ok(EXIT_OK)
        }
        Invocation::PlanExport {
            g,
            plan_id,
            destination,
            overwrite,
        } => {
            let spec = ExportSpec {
                plan_id,
                destination,
                overwrite_confirmed: overwrite,
                format: "csv_utf8_bom".to_string(),
            };
            let c = open(&g)?;
            let r: ExportReceipt = c.export_plan(&spec)?;
            if g.json {
                output::emit_json(out, &r);
            } else {
                output::emit_kv(out, &output::export_rows(&r));
            }
            Ok(EXIT_OK)
        }
        Invocation::PlanNote {
            g,
            plan_id,
            text,
            kind,
        } => {
            let note = ManualNote {
                plan_id,
                text,
                timestamp: nautilus_research_domain::time::now_rfc3339(),
                kind,
            };
            note.validate()?;
            let c = open(&g)?;
            let r: NoteRef = c.save_manual_note(&note)?;
            if g.json {
                output::emit_json(out, &r);
            } else {
                output::emit_kv(out, &output::note_rows(&r));
            }
            Ok(EXIT_OK)
        }
        Invocation::Snapshots { g, limit, cursor } => {
            let c = open(&g)?;
            let page: SnapshotPage = c.list_snapshots(limit, cursor.as_deref())?;
            if g.json {
                output::emit_json(out, &page);
            } else {
                output::emit_kv(out, &output::snapshots_rows(&page));
            }
            Ok(EXIT_OK)
        }
        Invocation::TaskGet { g, task } => {
            let c = open(&g)?;
            let v = c.get_task(&task)?;
            if g.json {
                output::emit_json(out, &v);
            } else {
                output::emit_kv(out, &output::task_view_rows(&v));
            }
            Ok(EXIT_OK)
        }
        Invocation::TaskWait { g, task, timeout } => {
            let c = open(&g)?;
            let v = c.wait_terminal(&task, Duration::from_secs(timeout))?;
            if g.json {
                output::emit_json(out, &v);
            } else {
                output::emit_kv(out, &output::task_view_rows(&v));
            }
            Ok(EXIT_OK)
        }
    }
}
