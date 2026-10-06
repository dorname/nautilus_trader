//! 桌面 ↔ 协调器桥：页面表单（纯数据）→ 类型化 spec → Coordinator 命令。
//!
//! 薄壳纪律与 CLI 同源：业务校验由协调器承载（ImportSpec::validate 等），
//! 本层只做机械转换；GUI 只发类型化消息、接收不可变视图（架构 core-05）。

use std::path::PathBuf;

use nautilus_research_domain::plan::{
    ExportReceipt, ExportSpec, HoldingInput, ManualNote, NoteRef,
};
use nautilus_research_domain::protocol::{
    CompareSpec, CompareView, Comparison, CostSpec, EffectiveRange, ImportSource, ImportSpec,
    Membership, MissingPolicy, PlanSpec, PriceBasis, Rebalance, RowsRow, RowsTable, RunSpec,
    SaveUniverseSpec, SnapshotPage, StrategySpec, StrategyTemplate, TaskRef, TaskView,
    UniverseMode, UniverseRef, UniverseSpec,
};
use nautilus_research_domain::universe::RuleGroup;
use nautilus_research_domain::worker_api::EquityDoc;
use nautilus_research_domain::{Coordinator, CoordinatorConfig, ResearchError};
use nautilus_research_worker::adapter;

type CmdResult<T> = Result<T, ResearchError>;

// ---------------------------------------------------------------- 表单（纯数据）

/// 导入表单。
#[derive(Debug, Clone)]
pub struct ImportForm {
    /// 0=tdx 1=tickflow 2=auxiliary。
    pub source: usize,
    /// 路径列表（每行一个）。
    pub paths: Vec<String>,
    /// 0=raw 1=qfq 2=hfq。
    pub price_basis: usize,
    /// auxiliary 时的种类（master/financial/actions/calendar/rules）。
    pub auxiliary_kind: Option<String>,
}

impl Default for ImportForm {
    fn default() -> Self {
        Self {
            source: 0,
            paths: Vec::new(),
            price_basis: 0,
            auxiliary_kind: None,
        }
    }
}

/// 股票池表单。
#[derive(Debug, Clone, Default)]
pub struct UniverseForm {
    /// 规则 JSON 文本（编辑态原样保存，提交时解析）。
    pub rule_text: String,
    pub as_of: String,
    pub strict: bool,
    pub fixed_membership: bool,
    pub ignore_missing: bool,
}

/// 运行表单。
#[derive(Debug, Clone)]
pub struct RunForm {
    pub fast: u32,
    pub slow: u32,
    pub top_k: u32,
    pub start: String,
    pub end: String,
    pub capital: String,
    pub commission_rate: String,
    pub min_commission: String,
    pub sell_tax: String,
    pub other_fee: String,
    pub slippage_bps: String,
    pub participation: String,
    pub eff_start: String,
    pub eff_end: String,
}

impl Default for RunForm {
    fn default() -> Self {
        Self {
            fast: 5,
            slow: 20,
            top_k: 10,
            start: String::new(),
            end: String::new(),
            capital: "1000000".into(),
            commission_rate: "0.0003".into(),
            min_commission: "5".into(),
            sell_tax: "0.001".into(),
            other_fee: "0.0002".into(),
            slippage_bps: "2".into(),
            participation: "0.1".into(),
            eff_start: "2020-01-01".into(),
            eff_end: "2030-12-31".into(),
        }
    }
}

/// 比较表单。
#[derive(Debug, Clone, Default)]
pub struct CompareForm {
    /// 运行 ID（逗号分割，2..5 个）。
    pub run_ids_text: String,
    pub intersection: bool,
}

/// 净值点（交易日 + 净值 CNY；RD-005：解析自 EquityDoc 十进制字符串）。
#[derive(Debug, Clone, PartialEq)]
pub struct EquityPoint {
    pub date: String,
    pub value: f64,
}

/// 计划表单。
#[derive(Debug, Clone, Default)]
pub struct PlanForm {
    pub as_of: String,
    /// 手工持仓 JSON（空 = 无持仓，用最近持有版本）。
    pub holdings_json: String,
    /// 导出目的地。
    pub export_path: String,
    pub note_text: String,
}

// ---------------------------------------------------------------- 桥

/// 桌面协调器桥：长驻协调器（单工作区单活跃协调器——CLI 批次 L1 结论）。
pub struct DesktopBridge {
    coordinator: Coordinator,
    pub workspace: PathBuf,
}

impl DesktopBridge {
    /// 打开工作区（进程内执行器注册；GUI 长驻协调器生命周期）。
    pub fn open(workspace: PathBuf) -> CmdResult<Self> {
        adapter::register_in_process();
        let coordinator = Coordinator::open(CoordinatorConfig::new(workspace.clone()))?;
        Ok(Self {
            coordinator,
            workspace,
        })
    }

    fn source(idx: usize) -> ImportSource {
        match idx {
            1 => ImportSource::Tickflow,
            2 => ImportSource::Auxiliary,
            _ => ImportSource::Tdx,
        }
    }

    fn price_basis(idx: usize) -> PriceBasis {
        match idx {
            1 => PriceBasis::Qfq,
            2 => PriceBasis::Hfq,
            _ => PriceBasis::Raw,
        }
    }

    /// 数据快照页：提交导入。
    pub fn submit_import(&self, form: &ImportForm) -> CmdResult<TaskRef> {
        let spec = ImportSpec {
            source: Self::source(form.source),
            paths: form.paths.clone(),
            symbols: None,
            start: None,
            end: None,
            price_basis: Self::price_basis(form.price_basis),
            auxiliary_kind: form.auxiliary_kind.clone(),
        };
        let key = format!(
            "gui-{}",
            &nautilus_research_domain::hash::hash_canonical(&spec)[..24]
        );
        self.coordinator
            .import_data(&format!("req-{key}"), &key, spec)
    }

    /// 数据快照页：快照清单。
    pub fn snapshots(&self, limit: u32) -> CmdResult<SnapshotPage> {
        self.coordinator.list_snapshots(Some(limit), None)
    }

    /// 股票池页：提交预览（规则 JSON 在此解析，错误转中文）。
    pub fn submit_preview(&self, snapshot_id: &str, form: &UniverseForm) -> CmdResult<TaskRef> {
        let rule: RuleGroup = serde_json::from_str(form.rule_text.trim()).map_err(|e| {
            ResearchError::invalid(format!("规则 JSON 解析失败：{e}")).with_field("rule")
        })?;
        let spec = UniverseSpec {
            snapshot_id: snapshot_id.to_string(),
            as_of: form.as_of.clone(),
            mode: if form.strict {
                UniverseMode::Strict
            } else {
                UniverseMode::Exploratory
            },
            membership: if form.fixed_membership {
                Membership::Fixed
            } else {
                Membership::Dynamic
            },
            rule,
            missing_policy: form
                .ignore_missing
                .then_some(MissingPolicy::IgnoreCondition),
        };
        let key = format!(
            "gui-{}",
            &nautilus_research_domain::hash::hash_canonical(&spec)[..24]
        );
        self.coordinator
            .preview_universe(&format!("req-{key}"), &key, spec)
    }

    /// 股票池页：保存版本。
    pub fn save_universe(
        &self,
        preview_task: &str,
        preview_hash: &str,
        input_hash: &str,
        name: &str,
    ) -> CmdResult<UniverseRef> {
        let spec = SaveUniverseSpec {
            preview_task_id: preview_task.to_string(),
            preview_hash: preview_hash.to_string(),
            input_hash: input_hash.to_string(),
            name: name.to_string(),
        };
        let key = format!(
            "gui-{}",
            &nautilus_research_domain::hash::hash_canonical(&spec)[..24]
        );
        self.coordinator
            .save_universe(&format!("req-{key}"), &key, spec)
    }

    /// 运行页：提交回测（EMA 模板）。
    pub fn submit_run(
        &self,
        snapshot_id: &str,
        universe_id: &str,
        form: &RunForm,
    ) -> CmdResult<TaskRef> {
        let spec = RunSpec {
            snapshot_id: snapshot_id.to_string(),
            universe_id: universe_id.to_string(),
            strategy: StrategySpec {
                template: StrategyTemplate::Ema,
                version: "1".into(),
                fast: Some(form.fast),
                slow: Some(form.slow),
                lookback: None,
                skip: None,
                top_k: form.top_k,
                rebalance: Rebalance::Daily,
                weight: "equal_slots".into(),
                signal_basis: "point_in_time_adjusted".into(),
            },
            start: form.start.clone(),
            end: form.end.clone(),
            training_end: None,
            test_start: None,
            capital_cny: form.capital.clone(),
            costs: self.costs(form),
            rules_hash: "0".repeat(64),
            benchmark_snapshot_id: None,
            mode: UniverseMode::Strict,
            grid: None,
            seed: 0,
        };
        let key = format!(
            "gui-{}",
            &nautilus_research_domain::hash::hash_canonical(&spec)[..24]
        );
        self.coordinator
            .submit_run(&format!("req-{key}"), &key, spec)
    }

    fn costs(&self, form: &RunForm) -> CostSpec {
        CostSpec {
            commission_rate: form.commission_rate.clone(),
            min_commission_cny: form.min_commission.clone(),
            sell_tax_rate: form.sell_tax.clone(),
            other_fee_rate: form.other_fee.clone(),
            slippage_bps: form.slippage_bps.clone(),
            participation_rate: form.participation.clone(),
            effective_schedule: vec![EffectiveRange {
                start: form.eff_start.clone(),
                end: form.eff_end.clone(),
            }],
            confirmed: true,
        }
    }

    /// 运行页：取消任务（长驻协调器下取消直达，GUI 形态的取消语义）。
    pub fn cancel(&self, task_id: &str) -> CmdResult<TaskView> {
        self.coordinator.cancel_task(task_id)
    }

    /// 任务查询（页面轮询；不存在时 NOT_FOUND）。
    pub fn get_task(&self, task_id: &str) -> CmdResult<TaskView> {
        self.coordinator.get_task(task_id)
    }

    /// 预览结果读取（预览任务成功后）。
    pub fn preview(
        &self,
        task_id: &str,
    ) -> CmdResult<nautilus_research_domain::protocol::UniversePreview> {
        self.coordinator.universe_preview(task_id)
    }

    /// 比较页：运行比较。
    pub fn compare(&self, form: &CompareForm, run_ids: Vec<String>) -> CmdResult<Comparison> {
        let spec = CompareSpec {
            run_ids,
            view: if form.intersection {
                CompareView::Intersection
            } else {
                CompareView::Full
            },
            benchmark_snapshot_id: None,
        };
        self.coordinator.compare_runs(&spec)
    }

    /// 实验页：读取已完成运行的净值曲线（RD-005：QueryRows equity 桶，分页拉全量）。
    /// run_id 即运行任务 ID（store 按 task_id 索引 research_run 行）。
    pub fn equity_curve(&self, run_id: &str) -> CmdResult<Vec<EquityPoint>> {
        let mut docs = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = self.coordinator.query_rows(
                run_id,
                RowsTable::Equity,
                Some(500),
                cursor.as_deref(),
            )?;
            for row in page.rows {
                if let RowsRow::Equity(d) = row {
                    docs.push(d);
                }
            }
            match page.next_cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        equity_points(docs)
    }

    /// 计划页：生成计划（手工持仓 JSON 在此解析）。
    pub fn submit_plan(
        &self,
        snapshot_id: &str,
        universe_id: &str,
        form: &PlanForm,
        run_form: &RunForm,
    ) -> CmdResult<TaskRef> {
        let holdings: Option<HoldingInput> = if form.holdings_json.trim().is_empty() {
            None
        } else {
            serde_json::from_str(form.holdings_json.trim()).map_err(|e| {
                ResearchError::invalid(format!("手工持仓 JSON 解析失败：{e}"))
                    .with_field("holdings")
            })?
        };
        let spec = PlanSpec {
            strategy: StrategySpec {
                template: StrategyTemplate::Ema,
                version: "1".into(),
                fast: Some(run_form.fast),
                slow: Some(run_form.slow),
                lookback: None,
                skip: None,
                top_k: run_form.top_k,
                rebalance: Rebalance::Daily,
                weight: "equal_slots".into(),
                signal_basis: "point_in_time_adjusted".into(),
            },
            snapshot_id: snapshot_id.to_string(),
            universe_id: universe_id.to_string(),
            as_of: form.as_of.clone(),
            holding_version_id: None,
            holdings,
            costs: self.costs(run_form),
            rules_hash: "0".repeat(64),
            allow_historical: false,
            mode: UniverseMode::Strict,
        };
        let key = format!(
            "gui-{}",
            &nautilus_research_domain::hash::hash_canonical(&spec)[..24]
        );
        self.coordinator
            .generate_plan(&format!("req-{key}"), &key, spec)
    }

    /// 计划页：导出 CSV。
    pub fn export_plan(&self, plan_id: &str, destination: &str) -> CmdResult<ExportReceipt> {
        let spec = ExportSpec {
            plan_id: plan_id.to_string(),
            destination: destination.to_string(),
            overwrite_confirmed: true,
            format: "csv_utf8_bom".into(),
        };
        self.coordinator.export_plan(&spec)
    }

    /// 计划页：保存备注。
    pub fn save_note(&self, plan_id: &str, text: &str, kind: &str) -> CmdResult<NoteRef> {
        let note = ManualNote {
            plan_id: plan_id.to_string(),
            text: text.to_string(),
            timestamp: nautilus_research_domain::time::now_rfc3339(),
            kind: kind.to_string(),
        };
        note.validate()?;
        self.coordinator.save_manual_note(&note)
    }

    /// 计划页：读取交易计划文档（核对视图）。
    pub fn trade_plan(
        &self,
        plan_id: &str,
    ) -> CmdResult<nautilus_research_domain::plan::TradePlanDoc> {
        self.coordinator.get_trade_plan(plan_id)
    }
}

/// 纯函数：EquityDoc 序列 → 绘制点列（UT-S15-10：日期原样保序；
/// 净值十进制字符串解析失败显式报错，不静默丢点）。
pub fn equity_points(docs: Vec<EquityDoc>) -> CmdResult<Vec<EquityPoint>> {
    docs.into_iter()
        .map(|d| {
            let value = d.equity_cny.trim().parse::<f64>().map_err(|_| {
                ResearchError::invalid(format!(
                    "净值无法解析：{}（{}）",
                    d.equity_cny, d.trade_date
                ))
                .with_field("equity_cny")
            })?;
            Ok(EquityPoint {
                date: d.trade_date,
                value,
            })
        })
        .collect()
}

/// AI 工作台计划桥：持仓 JSON →（现金, 总资产, 计划行）。
/// 行 =（代码, 参考价, 当前持有, 可卖, 目标, 调整数量）；调整 = 目标 − 当前。
///
/// ```json
/// {"cash_cny":"10000","total_assets_cny":"20000",
///  "positions":[{"instrument_id":"SYN-A","price":10,"quantity":1000,
///                "sellable_quantity":1000,"target_quantity":1100}]}
/// ```
pub fn parse_plan_rows(
    json: &str,
) -> Result<(f64, f64, Vec<(String, f64, i64, i64, i64, i64)>), String> {
    let v: serde_json::Value =
        serde_json::from_str(json.trim()).map_err(|e| format!("持仓 JSON 解析失败：{e}"))?;
    let num = |key: &str| -> Result<f64, String> {
        v.get(key)
            .and_then(|x| {
                x.as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .or_else(|| x.as_f64())
            })
            .ok_or_else(|| format!("{key} 缺失或非法"))
    };
    let cash = num("cash_cny")?;
    let total = num("total_assets_cny")?;
    let arr = v
        .get("positions")
        .and_then(|x| x.as_array())
        .ok_or("positions 缺失")?;
    let mut rows = Vec::new();
    for p in arr {
        let symbol = p
            .get("instrument_id")
            .and_then(|x| x.as_str())
            .ok_or("instrument_id 缺失")?
            .to_string();
        let price = p
            .get("price")
            .and_then(|x| x.as_f64())
            .ok_or("price 缺失")?;
        let qty = p
            .get("quantity")
            .and_then(|x| x.as_i64())
            .ok_or("quantity 缺失")?;
        let sellable = p
            .get("sellable_quantity")
            .and_then(|x| x.as_i64())
            .unwrap_or(qty);
        let target = p
            .get("target_quantity")
            .and_then(|x| x.as_i64())
            .unwrap_or(qty);
        rows.push((symbol, price, qty, sellable, target, target - qty));
    }
    Ok((cash, total, rows))
}
