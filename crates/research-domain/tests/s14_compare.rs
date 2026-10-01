//! S14 比较场景测试——批次 4（规格：logos/resources/test/core-S14-test-cases.md）。
//!
//! 覆盖：UT-S14-01（收益/回撤完整序列）、UT-S14-02（Sharpe 空值+原因），
//! UT-S14-03（无交集 NO_OVERLAP、基准缺日期）、UT-S14-04（结果篡改 CORRUPT_ARTIFACT），
//! ST-S14-01（费用差异显式列出、各自指标、不静默排名 + QueryRows 运行桶）。
//! 指标断言按完整净值序列（不降采样）；比较走真实 SubmitRun 产物。

use std::{fs, path::PathBuf, time::Duration};

use nautilus_research_domain::{
    metrics::{compute_metrics, NullableMetric},
    protocol::{
        CompareSpec, CompareView, CostSpec, EffectiveRange, Membership, Rebalance, RowsRow,
        RowsTable, RunSpec, StrategySpec, StrategyTemplate, UniverseMode, UniverseSpec,
    },
    universe::{Condition, RuleChild, RuleField, RuleGroup, RuleOp, AndOp},
    Coordinator, CoordinatorConfig, ErrorCode, ImportSource, ImportSpec, PriceBasis, TaskState,
};
use nautilus_research_testkit::case;
use nautilus_research_worker::adapter;
use rust_decimal::Decimal;

// ------------------------------------------------------------------ 测试夹具

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("research-s14-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn write_file(dir: &std::path::Path, name: &str, content: &str) -> String {
    let path = dir.join(name);
    fs::write(&path, content).expect("写入暂存文件");
    path.to_string_lossy().to_string()
}

fn quotes_csv(dir: &std::path::Path, name: &str, body: &str) -> String {
    write_file(
        dir,
        name,
        &format!("instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n{body}"),
    )
}

/// 24 个交易日（2023-12-04 起），可剔除若干日期（基准缺日期场景）。
fn rising_bars(code: &str, dir: &std::path::Path, name: &str, skip: &[&str]) -> String {
    let dates = [
        "2023-12-04", "2023-12-05", "2023-12-06", "2023-12-07", "2023-12-08",
        "2023-12-11", "2023-12-12", "2023-12-13", "2023-12-14", "2023-12-15",
        "2023-12-18", "2023-12-19", "2023-12-20", "2023-12-21", "2023-12-22",
        "2023-12-25", "2023-12-26", "2023-12-27", "2023-12-28", "2023-12-29",
        "2024-01-02", "2024-01-03", "2024-01-04", "2024-01-05",
    ];
    let mut body = String::new();
    for (i, d) in dates.iter().enumerate() {
        if skip.contains(d) {
            continue;
        }
        let close = 10 + i as i32;
        body.push_str(&format!("{code},{d},{close},{close},{close},{close},100000,{close}0000\n"));
    }
    quotes_csv(dir, name, &body)
}

fn open_coordinator(tag: &str) -> (Coordinator, PathBuf) {
    let ws = temp_workspace(tag);
    adapter::register_in_process();
    let c = Coordinator::open(CoordinatorConfig::new(ws.clone())).expect("打开协调器");
    (c, ws)
}

fn import_ok(c: &Coordinator, key: &str, paths: Vec<String>) -> String {
    let spec = ImportSpec {
        source: ImportSource::Auxiliary,
        paths,
        symbols: None,
        start: None,
        end: None,
        price_basis: PriceBasis::Raw,
        auxiliary_kind: None,
    };
    let task_ref = c.import_data(&format!("req-{key}"), key, spec).expect("ImportData 入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
        .expect("等待导入终态");
    assert_eq!(view.state, TaskState::Succeeded, "导入应成功：{:?}", view.error);
    view.snapshot_id.expect("成功任务必须有 snapshot_id")
}

fn save_universe_all(c: &Coordinator, key: &str, snapshot_id: &str) -> String {
    let rule = RuleGroup {
        op: AndOp::And,
        children: vec![RuleChild::Cond(Condition {
            field: RuleField::Close,
            op: RuleOp::Gte,
            value: Some("0".to_string()),
            values: None,
            window: None,
        })],
    };
    let spec = UniverseSpec {
        snapshot_id: snapshot_id.to_string(),
        as_of: "2024-01-05".to_string(),
        mode: UniverseMode::Strict,
        membership: Membership::Fixed,
        rule,
        missing_policy: None,
    };
    let task_ref = c
        .preview_universe(&format!("req-{key}-p"), &format!("{key}-p"), spec)
        .expect("预览入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
        .expect("等待预览终态");
    assert_eq!(view.state, TaskState::Succeeded, "预览应成功：{:?}", view.error);
    let preview = c.universe_preview(&task_ref.task_id).expect("预览摘要");
    let save = nautilus_research_domain::protocol::SaveUniverseSpec {
        preview_task_id: task_ref.task_id.clone(),
        preview_hash: preview.preview_hash.clone(),
        input_hash: preview.input_hash.clone(),
        name: format!("池-{key}"),
    };
    let uni = c
        .save_universe(&format!("req-{key}-s"), &format!("{key}-s"), save)
        .expect("保存股票池");
    uni.universe_id
}

fn costs(rate: &str, min: &str) -> CostSpec {
    CostSpec {
        commission_rate: rate.to_string(),
        min_commission_cny: min.to_string(),
        sell_tax_rate: "0".to_string(),
        other_fee_rate: "0".to_string(),
        slippage_bps: "0".to_string(),
        participation_rate: "1".to_string(),
        effective_schedule: vec![EffectiveRange {
            start: "2023-12-01".to_string(),
            end: "2024-02-01".to_string(),
        }],
        confirmed: true,
    }
}

fn run_spec(snapshot_id: &str, universe_id: &str, start: &str, end: &str, rate: &str, min: &str) -> RunSpec {
    RunSpec {
        snapshot_id: snapshot_id.to_string(),
        universe_id: universe_id.to_string(),
        strategy: StrategySpec {
            template: StrategyTemplate::Ema,
            version: "1".to_string(),
            fast: Some(1),
            slow: Some(2),
            lookback: None,
            skip: None,
            top_k: 1,
            rebalance: Rebalance::Daily,
            weight: "equal_slots".to_string(),
            signal_basis: "point_in_time_adjusted".to_string(),
        },
        start: start.to_string(),
        end: end.to_string(),
        training_end: None,
        test_start: None,
        capital_cny: "20000".to_string(),
        costs: costs(rate, min),
        rules_hash: "0".repeat(64),
        benchmark_snapshot_id: None,
        mode: UniverseMode::Strict,
        grid: None,
        seed: 0,
    }
}

/// 提交一次运行并等待成功，返回 task_id。
fn submit_ok(c: &Coordinator, key: &str, spec: &RunSpec) -> String {
    let task_ref = c
        .submit_run(&format!("req-{key}"), key, spec.clone())
        .expect("SubmitRun 入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(20))
        .expect("运行终态");
    assert_eq!(view.state, TaskState::Succeeded, "运行应成功：{:?}", view.error);
    task_ref.task_id
}

// ------------------------------------------------------------------ 用例

/// UT-S14-01：净值 100、110、99，初始 100——总收益 -0.01、最大回撤 0.1；
/// 按完整序列计算（升序，不按降采样曲线），无 NaN/Infinity。
#[test]
fn ut_s14_01_total_return_and_max_drawdown_full_series() {
    case("UT-S14-01", || {
        let equity = vec![
            Decimal::from(100),
            Decimal::from(110),
            Decimal::from(99),
        ];
        let m = compute_metrics(&equity, Decimal::from(100), Decimal::ZERO, Decimal::ZERO);
        assert_eq!(m.total_return.value.as_deref(), Some("-0.01"), "总收益");
        assert_eq!(m.max_drawdown.value.as_deref(), Some("0.1"), "最大回撤 (110-99)/110");
        // 完整序列：两个收益样本 → 波动/Sharpe 可算且为规范十进制（无 NaN）
        let vol = m.volatility.value.as_deref().expect("波动可算");
        let sharpe = m.sharpe.value.as_deref().expect("Sharpe 可算");
        vol.parse::<Decimal>().expect("波动为十进制");
        sharpe.parse::<Decimal>().expect("Sharpe 为十进制");
        // 乱序输入按日期排序是调用方职责；此处直接验证降采样会失真的场景：
        // 中间峰值 110 只在完整序列中产生 0.1 回撤（若丢弃中间点回撤为 0）
        let sampled = vec![Decimal::from(100), Decimal::from(99)];
        let ms = compute_metrics(&sampled, Decimal::from(100), Decimal::ZERO, Decimal::ZERO);
        assert_eq!(ms.max_drawdown.value.as_deref(), Some("0.01"), "降采样序列回撤不同，证明按完整序列计算");
        assert_eq!(m.cost_cny.value.as_deref(), Some("0"));
    });
}

/// UT-S14-02：所有日收益为 0 或仅 1 个收益样本——Sharpe 为空并附原因，
/// 无 NaN/Infinity（可空指标显式建模）。
#[test]
fn ut_s14_02_shape_empty_with_reason_when_not_computable() {
    case("UT-S14-02", || {
        // 全零收益 → 波动为零
        let flat = vec![Decimal::from(100), Decimal::from(100), Decimal::from(100)];
        let m = compute_metrics(&flat, Decimal::from(100), Decimal::ZERO, Decimal::ZERO);
        assert_eq!(m.sharpe.value, None, "零波动 Sharpe 必须为空");
        assert!(m.sharpe.reason.as_deref().unwrap_or_default().contains("波动为零"));
        // 仅 1 个收益样本
        let short = vec![Decimal::from(100), Decimal::from(110)];
        let m1 = compute_metrics(&short, Decimal::from(100), Decimal::ZERO, Decimal::ZERO);
        assert_eq!(m1.sharpe.value, None, "单样本 Sharpe 必须为空");
        assert!(m1.sharpe.reason.as_deref().unwrap_or_default().contains("样本"));
        assert_eq!(m1.volatility.value, None);
        // 可空指标结构：value 与 reason 互斥成立，绝不含 NaN 字面量
        let probe = NullableMetric::none("x");
        assert!(probe.value.is_none() && probe.reason.is_some());
        let total = m.total_return.value.as_deref().expect("平净值总收益可算");
        assert_eq!(total, "0");
    });
}

/// UT-S14-03：两实验区间不相交 → 交集请求 NO_OVERLAP；
/// 基准缺一个日期 → 基准相关指标为空并附原因，策略指标保留。
#[test]
fn ut_s14_03_no_overlap_and_benchmark_missing_date() {
    case("UT-S14-03", || {
        let (c, ws) = open_coordinator("ut03");
        let quotes = rising_bars("SYN-A", &ws, "a.csv", &[]);
        let snapshot_id = import_ok(&c, "s14-03", vec![quotes]);
        let universe_id = save_universe_all(&c, "s14-03", &snapshot_id);

        // 区间不相交的两个运行
        let a = run_spec(&snapshot_id, &universe_id, "2023-12-04", "2023-12-15", "0", "0");
        let b = run_spec(&snapshot_id, &universe_id, "2023-12-18", "2024-01-05", "0", "0");
        let run_a = submit_ok(&c, "s14-03-a", &a);
        let run_b = submit_ok(&c, "s14-03-b", &b);

        // 交集请求 → NO_OVERLAP
        let err = c
            .compare_runs(&CompareSpec {
                run_ids: vec![run_a.clone(), run_b.clone()],
                view: CompareView::Intersection,
                benchmark_snapshot_id: None,
            })
            .expect_err("无交集必须拒绝");
        assert_eq!(err.code, ErrorCode::NoOverlap, "{err:?}");
        // 完整视图允许（不静默拒绝）
        let full = c
            .compare_runs(&CompareSpec {
                run_ids: vec![run_a.clone(), run_b],
                view: CompareView::Full,
                benchmark_snapshot_id: None,
            })
            .expect("完整视图无交集也返回");
        assert!(full.overlap.is_none(), "无交集时 overlap 为空");

        // 基准缺一个日期（剔除 2023-12-06）：基准指标全空，策略指标保留
        let bench_quotes = rising_bars("SYN-A", &ws, "bench.csv", &["2023-12-06"]);
        let bench_snapshot = import_ok(&c, "s14-03-bench", vec![bench_quotes]);
        // 同配置再跑一个运行 C（与 A 同窗口，新幂等键）
        let run_c = submit_ok(&c, "s14-03-c", &a);
        let cmp = c
            .compare_runs(&CompareSpec {
                run_ids: vec![run_a.clone(), run_c],
                view: CompareView::Intersection,
                benchmark_snapshot_id: Some(bench_snapshot),
            })
            .expect("有交集比较应成功");
        let bench = cmp.benchmark.expect("基准视图存在");
        assert_eq!(bench.metrics.total_return.value, None, "基准缺日期指标为空");
        assert!(bench.metrics.total_return.reason.as_deref().unwrap_or_default().contains("基准缺"));
        for r in &cmp.runs {
            assert!(r.metrics.total_return.value.is_some(), "策略指标保留");
            assert!(r.metrics.max_drawdown.value.is_some(), "策略指标保留");
        }
        let overlap = cmp.overlap.expect("有交集");
        assert_eq!((overlap.start.as_str(), overlap.end.as_str()), ("2023-12-04", "2023-12-15"));
    });
}

/// UT-S14-04：对结果文件修改一个价格但保持旧清单 → CORRUPT_ARTIFACT；
/// 不把缓存当作新结果展示。
#[test]
fn ut_s14_04_tampered_result_is_corrupt_artifact() {
    case("UT-S14-04", || {
        let (c, ws) = open_coordinator("ut04");
        let quotes = rising_bars("SYN-A", &ws, "a.csv", &[]);
        let snapshot_id = import_ok(&c, "s14-04", vec![quotes]);
        let universe_id = save_universe_all(&c, "s14-04", &snapshot_id);
        let spec = run_spec(&snapshot_id, &universe_id, "2023-12-04", "2024-01-05", "0", "0");
        let run_a = submit_ok(&c, "s14-04-a", &spec);

        // 篡改前可查询，并取得结果对象哈希
        let page = c
            .query_rows(&run_a, RowsTable::Equity, None, None)
            .expect("篡改前查询成功");
        let hash = page.object_hash.clone();
        assert!(!page.rows.is_empty(), "净值行非空");

        // 修改结果文件内容（保持旧清单/旧哈希引用）
        let rel = nautilus_research_domain::objects::ObjectStore::relative_path(&hash);
        let path = ws.join(&rel);
        let mut bytes = fs::read(&path).expect("读取结果对象");
        bytes.push(b'x');
        fs::write(&path, &bytes).expect("篡改结果对象");

        // 比较：CORRUPT_ARTIFACT（不展示缓存为新结果）
        let other = submit_ok(&c, "s14-04-b", &spec);
        let err = c
            .compare_runs(&CompareSpec {
                run_ids: vec![run_a.clone(), other],
                view: CompareView::Intersection,
                benchmark_snapshot_id: None,
            })
            .expect_err("篡改结果必须拒绝");
        assert_eq!(err.code, ErrorCode::CorruptArtifact, "{err:?}");
        // 查询同样拒绝
        let err = c
            .query_rows(&run_a, RowsTable::Equity, None, None)
            .expect_err("篡改后查询必须拒绝");
        assert_eq!(err.code, ErrorCode::CorruptArtifact);
    });
}

/// ST-S14-01：不同费用的两个已完成实验——列出费用差异、分别展示指标，
/// 不无提示直接排名；顺带验证 QueryRows 运行三桶（equity/holdings/fills）。
#[test]
fn st_s14_01_compare_cost_difference_and_run_buckets() {
    case("ST-S14-01", || {
        let (c, ws) = open_coordinator("st01");
        let quotes = rising_bars("SYN-A", &ws, "a.csv", &[]);
        let snapshot_id = import_ok(&c, "s14-st01", vec![quotes]);
        let universe_id = save_universe_all(&c, "s14-st01", &snapshot_id);
        let free = run_spec(&snapshot_id, &universe_id, "2023-12-04", "2024-01-05", "0", "0");
        let fee = run_spec(&snapshot_id, &universe_id, "2023-12-04", "2024-01-05", "0.001", "5");
        let run_a = submit_ok(&c, "s14-st01-a", &free);
        let run_b = submit_ok(&c, "s14-st01-b", &fee);
        for id in [&run_a, &run_b] {
            if let Err(e) = c.query_rows(id, RowsTable::Equity, None, None) {
                panic!("运行 {id} 产物查询失败：{e:?}");
            }
        }

        let cmp = c
            .compare_runs(&CompareSpec {
                run_ids: vec![run_a.clone(), run_b.clone()],
                view: CompareView::Intersection,
                benchmark_snapshot_id: None,
            })
            .expect("比较成功");
        // 费用差异显式列出（含两个费率）
        let cost_diff = cmp
            .differences
            .iter()
            .find(|d| d.kind == "cost")
            .expect("必须列出费用差异");
        assert!(cost_diff.detail.contains("0.001") && cost_diff.detail.contains(&run_a) && cost_diff.detail.contains(&run_b), "{:?}",
            cost_diff.detail);
        // 分别展示指标（两行、各自 total_return），无排名字段
        assert_eq!(cmp.runs.len(), 2, "每个运行单独一行指标");
        for r in &cmp.runs {
            assert!(r.metrics.total_return.value.is_some(), "指标可算：{:?}", r.metrics.total_return);
            assert!(r.metrics.sharpe.value.is_some() || r.metrics.sharpe.reason.is_some());
        }
        assert!(cmp.overlap.is_some());
        let _ = &cmp; // Comparison 结构无 rank 字段（编译期保证不静默排名）

        // QueryRows 运行桶：equity / holdings / fills 类型化行 + 分页与游标绑定
        let equity = c.query_rows(&run_a, RowsTable::Equity, Some(2), None).expect("净值桶");
        assert_eq!(equity.rows.len(), 2);
        assert!(equity.next_cursor.is_some(), "分页游标存在");
        let first = match &equity.rows[0] {
            RowsRow::Equity(e) => e.clone(),
            other => panic!("应为净值行：{other:?}"),
        };
        assert!(!first.trade_date.is_empty() && !first.equity_cny.is_empty());
        assert!(!first.cash_cny.is_empty() && !first.positions_value_cny.is_empty());
        // 跨版本游标拒绝
        let bad_cursor = format!("{}:2", "0".repeat(64));
        let err = c
            .query_rows(&run_a, RowsTable::Equity, Some(2), Some(&bad_cursor))
            .expect_err("游标哈希不符必须拒绝");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        // 续页
        let next = c
            .query_rows(&run_a, RowsTable::Equity, Some(2), equity.next_cursor.as_deref())
            .expect("续页成功");
        assert!(!next.rows.is_empty());
        assert_eq!(next.object_hash, equity.object_hash);

        let fills = c.query_rows(&run_b, RowsTable::Fills, None, None).expect("成交桶");
        assert!(!fills.rows.is_empty(), "收费运行有成交");
        let fill = match &fills.rows[0] {
            RowsRow::Fill(f) => f.clone(),
            other => panic!("应为成交行：{other:?}"),
        };
        assert!(fill.fill_id.starts_with("F-"), "fill_id 规范化");
        assert_eq!(fill.side, "buy");

        let holdings = c.query_rows(&run_a, RowsTable::Holdings, None, None).expect("持仓桶");
        assert!(!holdings.rows.is_empty());
        let h = match holdings.rows.last().expect("持仓行") {
            RowsRow::Holding(h) => h.clone(),
            other => panic!("应为持仓行：{other:?}"),
        };
        assert!(h.quantity >= h.sellable_quantity, "可卖数量不超过持仓（T+1）");
    });
}
