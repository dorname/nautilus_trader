//! S15 交易计划场景测试——批次 5（规格：logos/resources/test/core-S15-test-cases.md）。
//!
//! 覆盖：UT-S15-01（目标仓位数学）、UT-S15-02（持仓校验定位字段）、
//! UT-S15-03（CSV 转义/中文空格路径/PATH_CONFLICT）、UT-S15-04（历史计划 STALE_DATA）、
//! ST-S15-01（生成→导出→备注全旅程，fills 不变）。
//! 计划是纯参考产物：全链路无下单通道调用（类型层无 broker API，编译期保证）。

use std::{fs, path::PathBuf, time::Duration};

use nautilus_research_domain::{
    plan::{csv_escape, check_stale_as_of, plan_target, ExportSpec, HoldingInput, ManualNote, PositionInput},
    protocol::{
        CostSpec, EffectiveRange, Membership, Rebalance, RowsTable, RunSpec, StrategySpec,
        StrategyTemplate, UniverseMode, UniverseSpec,
    },
    universe::{Condition, RuleChild, RuleField, RuleGroup, RuleOp, AndOp},
    Coordinator, CoordinatorConfig, ErrorCode, ImportSource, ImportSpec, PriceBasis, TaskState,
};
use nautilus_research_testkit::{case, report_result};
use nautilus_research_worker::adapter;
use rust_decimal::Decimal;

// ------------------------------------------------------------------ 测试夹具

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("research-s15-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn quotes_csv(dir: &std::path::Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    fs::write(
        &path,
        format!("instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n{body}"),
    )
    .expect("写入暂存行情");
    path.to_string_lossy().to_string()
}

/// 24 个交易日（2023-12-04 起）单边上行行情，末日 2024-01-05 收盘 33。
fn rising_bars(code: &str, dir: &std::path::Path, name: &str) -> String {
    let dates = [
        "2023-12-04", "2023-12-05", "2023-12-06", "2023-12-07", "2023-12-08",
        "2023-12-11", "2023-12-12", "2023-12-13", "2023-12-14", "2023-12-15",
        "2023-12-18", "2023-12-19", "2023-12-20", "2023-12-21", "2023-12-22",
        "2023-12-25", "2023-12-26", "2023-12-27", "2023-12-28", "2023-12-29",
        "2024-01-02", "2024-01-03", "2024-01-04", "2024-01-05",
    ];
    let mut body = String::new();
    for (i, d) in dates.iter().enumerate() {
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

fn run_spec(snapshot_id: &str, universe_id: &str) -> RunSpec {
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
        start: "2023-12-04".to_string(),
        end: "2024-01-05".to_string(),
        training_end: None,
        test_start: None,
        capital_cny: "20000".to_string(),
        costs: costs("0", "0"),
        rules_hash: "0".repeat(64),
        benchmark_snapshot_id: None,
        mode: UniverseMode::Strict,
        grid: None,
        seed: 0,
    }
}

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

/// 提交计划生成并等待终态（供成功路径复用）。
fn generate_plan_ok(c: &Coordinator, key: &str, spec: nautilus_research_domain::protocol::PlanSpec) -> String {
    let task_ref = c
        .generate_plan(&format!("req-{key}"), key, spec)
        .expect("GeneratePlan 入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
        .expect("计划终态");
    assert_eq!(view.state, TaskState::Succeeded, "计划应成功：{:?}", view.error);
    task_ref.task_id
}

fn sample_holdings() -> HoldingInput {
    HoldingInput {
        as_of: "2024-01-05".to_string(),
        cash_cny: "1000".to_string(),
        positions: vec![PositionInput {
            instrument_id: "SYN-A".to_string(),
            quantity: 100,
            sellable_quantity: 100,
        }],
        total_assets_cny: "20000".to_string(),
    }
}

// ------------------------------------------------------------------ 用例

/// UT-S15-01：总资产 10000、权重 0.2、价 10、持 100 可卖 100、步长 100——
/// 目标 200 股、建议买 100 股、参考额 1000；计划是纯参考产物，无券商调用通道。
#[test]
fn ut_s15_01_plan_target_math() {
    case("UT-S15-01", || {
        let t = plan_target(
            Decimal::from(10000),
            Decimal::new(2, 1), // 0.2
            Decimal::from(10),
            100,
            100,
        )
        .expect("目标仓位计算");
        assert_eq!(t.target_quantity, 200, "floor(10000×0.2/10/100)×100=200");
        assert_eq!(t.delta_quantity, 100, "目标 200 - 持有 100 = 建议买 100");
        assert_eq!(t.reference_value_cny, Decimal::from(1000), "100 股 × 10 元");
        // 已达目标：差额为 0，参考额为 0
        let t0 = plan_target(Decimal::from(10000), Decimal::new(2, 1), Decimal::from(10), 200, 100)
            .expect("已达目标计算");
        assert_eq!(t0.delta_quantity, 0, "持有即目标");
        assert_eq!(t0.reference_value_cny, Decimal::ZERO);
        // 卖出方向：持有 300 → 建议卖 100，参考额 1000
        let ts = plan_target(Decimal::from(10000), Decimal::new(2, 1), Decimal::from(10), 300, 100)
            .expect("卖出方向计算");
        assert_eq!(ts.delta_quantity, -100);
        assert_eq!(ts.reference_value_cny, Decimal::from(1000));
        // 纯函数无副作用：PlanTarget 仅含数学结果字段（股数/差额/参考额），
        // 无订单/委托句柄——全 crate 不存在 broker/order 通道类型（FR-R09 编译期保证）
    });
}

/// UT-S15-02：可卖量 200 超过持有 100，或现金为负——拒绝生成并定位字段。
#[test]
fn ut_s15_02_holdings_validation_locates_field() {
    case("UT-S15-02", || {
        let (c, ws) = open_coordinator("ut02");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let snapshot_id = import_ok(&c, "s15-02", vec![quotes]);
        let universe_id = save_universe_all(&c, "s15-02", &snapshot_id);

        // 可卖 > 持有：定位 positions[0].sellable_quantity
        let bad = HoldingInput {
            as_of: "2024-01-05".to_string(),
            cash_cny: "1000".to_string(),
            positions: vec![PositionInput {
                instrument_id: "SYN-A".to_string(),
                quantity: 100,
                sellable_quantity: 200,
            }],
            total_assets_cny: "20000".to_string(),
        };
        let spec = nautilus_research_domain::protocol::PlanSpec {
            strategy: run_spec(&snapshot_id, &universe_id).strategy,
            snapshot_id,
            universe_id,
            as_of: "2024-01-05".to_string(),
            holding_version_id: None,
            holdings: Some(bad),
            costs: costs("0", "0"),
            rules_hash: "0".repeat(64),
            allow_historical: false,
            mode: UniverseMode::Strict,
        };
        let err = c
            .generate_plan("req-s15-02-a", "s15-02-a", spec.clone())
            .expect_err("可卖超持必须拒绝");
        assert_eq!(err.code, ErrorCode::InvalidArgument, "{err:?}");
        assert_eq!(
            err.field.as_deref(),
            Some("positions[0].sellable_quantity"),
            "必须定位字段：{err:?}"
        );

        // 现金为负：定位 cash_cny
        let mut neg = spec.clone();
        if let Some(h) = &mut neg.holdings {
            h.positions.clear();
            h.cash_cny = "-1".to_string();
        }
        let err = c
            .generate_plan("req-s15-02-b", "s15-02-b", neg)
            .expect_err("负现金必须拒绝");
        assert_eq!(err.field.as_deref(), Some("cash_cny"), "必须定位现金字段：{err:?}");
    });
}

/// UT-S15-03：名称以 = 开头的自由文本转义；含中文与空格的导出路径正确；
/// 目标文件已存在且未确认覆盖 → PATH_CONFLICT。
#[test]
fn ut_s15_03_csv_escape_and_export_path_conflict() {
    case("UT-S15-03", || {
        // CSV 自由文本转义：公式前缀加单引号并引号包裹
        assert_eq!(csv_escape("=1+1"), "\"'=1+1\"", "公式前缀防注入");
        assert_eq!(csv_escape("+SUM(A1)"), "\"'+SUM(A1)\"");
        assert_eq!(csv_escape("普通文本"), "普通文本", "普通文本不转义");
        assert_eq!(csv_escape("含,逗号"), "\"含,逗号\"", "分隔符引号包裹");
        assert_eq!(csv_escape("引\"号"), "\"引\"\"号\"", "引号双写");

        let (c, ws) = open_coordinator("ut03");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let snapshot_id = import_ok(&c, "s15-03", vec![quotes]);
        let universe_id = save_universe_all(&c, "s15-03", &snapshot_id);
        let base = run_spec(&snapshot_id, &universe_id);
        let plan_id = generate_plan_ok(
            &c,
            "s15-03-plan",
            nautilus_research_domain::protocol::PlanSpec {
                strategy: base.strategy,
                snapshot_id,
                universe_id,
                as_of: "2024-01-05".to_string(),
                holding_version_id: None,
                holdings: Some(sample_holdings()),
                costs: costs("0", "0"),
                rules_hash: "0".repeat(64),
                allow_historical: false,
                mode: UniverseMode::Strict,
            },
        );

        // 含中文与空格的导出路径
        let dest_dir = ws.join("导出 目录");
        let dest = dest_dir.join("交易 计划.csv");
        let receipt = c
            .export_plan(&ExportSpec {
                plan_id: plan_id.clone(),
                destination: dest.to_string_lossy().to_string(),
                overwrite_confirmed: false,
                format: "csv_utf8_bom".to_string(),
            })
            .expect("中文空格路径导出成功");
        assert!(dest.exists(), "导出文件落在指定路径");
        assert_eq!(receipt.rows, 1, "数据行数（不含表头）");
        let on_disk = fs::read(&dest).expect("读回导出文件");
        assert_eq!(receipt.sha256, nautilus_research_domain::hash::sha256_hex(&on_disk));

        // 已存在且未确认覆盖 → PATH_CONFLICT
        let err = c
            .export_plan(&ExportSpec {
                plan_id: plan_id.clone(),
                destination: dest.to_string_lossy().to_string(),
                overwrite_confirmed: false,
                format: "csv_utf8_bom".to_string(),
            })
            .expect_err("未确认覆盖必须拒绝");
        assert_eq!(err.code, ErrorCode::PathConflict, "{err:?}");
        // 确认后覆盖成功
        let receipt2 = c
            .export_plan(&ExportSpec {
                plan_id,
                destination: dest.to_string_lossy().to_string(),
                overwrite_confirmed: true,
                format: "csv_utf8_bom".to_string(),
            })
            .expect("确认后覆盖成功");
        assert_eq!(receipt2.sha256, receipt.sha256, "内容确定则哈希一致");
    });
}

/// UT-S15-04：as_of 早于最近已结束交易日——默认 STALE_DATA；
/// 显式确认后计划与 CSV 均含历史计划标识。
#[test]
fn ut_s15_04_historical_plan_requires_confirmation() {
    case("UT-S15-04", || {
        // 纯函数层：默认拒绝、显式确认放行、当日不算历史
        let err = check_stale_as_of("2024-01-02", "2024-01-05", false).expect_err("默认拒绝历史计划");
        assert_eq!(err.code, ErrorCode::StaleData, "{err:?}");
        assert_eq!(err.field.as_deref(), Some("as_of"));
        assert!(check_stale_as_of("2024-01-02", "2024-01-05", true).expect("确认后放行"));
        assert!(!check_stale_as_of("2024-01-05", "2024-01-05", false).expect("当日非历史"));

        // 端到端：as_of=2024-01-04 早于快照覆盖末日 2024-01-05
        let (c, ws) = open_coordinator("ut04");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let snapshot_id = import_ok(&c, "s15-04", vec![quotes]);
        let universe_id = save_universe_all(&c, "s15-04", &snapshot_id);
        let base = run_spec(&snapshot_id, &universe_id);
        let mk = |allow: bool| nautilus_research_domain::protocol::PlanSpec {
            strategy: base.strategy.clone(),
            snapshot_id: snapshot_id.clone(),
            universe_id: universe_id.clone(),
            as_of: "2024-01-04".to_string(),
            holding_version_id: None,
            holdings: Some(sample_holdings()),
            costs: costs("0", "0"),
            rules_hash: "0".repeat(64),
            allow_historical: allow,
            mode: UniverseMode::Strict,
        };

        // 默认：任务失败 STALE_DATA
        let task_ref = c
            .generate_plan("req-s15-04-a", "s15-04-a", mk(false))
            .expect("入队成功");
        let view = c
            .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
            .expect("等待终态");
        assert_eq!(view.state, TaskState::Failed, "未确认的历史计划必须失败");
        assert_eq!(view.error.expect("失败必须含错误").code, "STALE_DATA");

        // 显式确认：成功且计划/CSV 均含历史标识
        let plan_id = generate_plan_ok(&c, "s15-04-b", mk(true));
        let doc = c.get_trade_plan(&plan_id).expect("读取计划");
        assert!(doc.historical, "计划必须标记历史");
        let csv = String::from_utf8(doc.csv_bytes()).expect("CSV 为 UTF-8");
        assert!(csv.contains("historical-plan"), "CSV 必须含历史计划标识：{csv}");
    });
}

/// ST-S15-01：GeneratePlan → GetTask → ExportPlan → SaveManualNote 全旅程——
/// 导出哈希一致、股数正确、数据版本与限制齐全；备注不更改回测 fills。
#[test]
fn st_s15_01_plan_export_note_end_to_end() {
    case("ST-S15-01", || {
        let (c, ws) = open_coordinator("st01");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let snapshot_id = import_ok(&c, "s15-st01", vec![quotes]);
        let universe_id = save_universe_all(&c, "s15-st01", &snapshot_id);

        // 先跑一次回测，留取 fills 基线（备注不得改动）
        let run_id = submit_ok(&c, "s15-st01-run", &run_spec(&snapshot_id, &universe_id));
        let fills_before = c
            .query_rows(&run_id, RowsTable::Fills, None, None)
            .expect("回测 fills 查询");
        let fill_ids_before: Vec<String> = fills_before
            .rows
            .iter()
            .map(|r| match r {
                nautilus_research_domain::protocol::RowsRow::Fill(f) => f.fill_id.clone(),
                other => panic!("应为成交行：{other:?}"),
            })
            .collect();
        assert!(!fill_ids_before.is_empty(), "回测应有成交");

        // 生成计划（持 100 股、总资产 20000、参考价 33 → 目标 floor(20000/33/100)×100=600）
        let base = run_spec(&snapshot_id, &universe_id);
        let plan_id = generate_plan_ok(
            &c,
            "s15-st01-plan",
            nautilus_research_domain::protocol::PlanSpec {
                strategy: base.strategy,
                snapshot_id: snapshot_id.clone(),
                universe_id: universe_id.clone(),
                as_of: "2024-01-05".to_string(),
                holding_version_id: None,
                holdings: Some(sample_holdings()),
                costs: costs("0", "0"),
                rules_hash: "0".repeat(64),
                allow_historical: false,
                mode: UniverseMode::Strict,
            },
        );
        let doc = c.get_trade_plan(&plan_id).expect("GetTask 后读取计划");
        assert!(!doc.historical, "当日计划非历史");
        assert_eq!(doc.rows.len(), 1);
        let row = &doc.rows[0];
        assert_eq!(row.instrument_id, "SYN-A");
        assert_eq!(row.reference_price, "33", "参考价为 as_of 收盘价");
        assert_eq!(row.target_quantity, 600, "floor(20000×1/33/100)×100=600");
        assert_eq!(row.delta_quantity, 500, "600 - 持有 100");
        assert_eq!(row.target_weight, "1", "单成员单槽等权");
        assert_eq!(doc.total_assets_cny, "20000");
        assert_eq!(doc.cash_cny, "1000");
        // 数据版本与限制齐全
        assert_eq!(doc.data_version.len(), 64, "数据版本为快照清单哈希");
        assert!(
            doc.limitations.iter().any(|l| l.contains("人工执行")),
            "必须声明仅人工执行：{:?}",
            doc.limitations
        );

        // 导出：回执哈希与文件内容一致，行数正确
        let dest = ws.join("导出/计划.csv");
        let receipt = c
            .export_plan(&ExportSpec {
                plan_id: plan_id.clone(),
                destination: dest.to_string_lossy().to_string(),
                overwrite_confirmed: false,
                format: "csv_utf8_bom".to_string(),
            })
            .expect("导出成功");
        let on_disk = fs::read(&dest).expect("读回导出文件");
        assert_eq!(receipt.sha256, nautilus_research_domain::hash::sha256_hex(&on_disk), "导出哈希一致");
        assert_eq!(receipt.rows, doc.rows.len() as u64, "导出行数与计划行数一致");
        let head = String::from_utf8_lossy(&on_disk);
        assert!(head.starts_with('\u{FEFF}'), "UTF-8 BOM");
        assert!(head.contains("SYN-A") && head.contains("600"), "导出含代码与目标股数");
        assert!(head.contains("人工执行"), "导出必含限制说明");

        // 追加人工备注：不更改回测 fills
        let note = c
            .save_manual_note(&ManualNote {
                plan_id: plan_id.clone(),
                text: "已人工核对流动性，按半量执行".to_string(),
                timestamp: "2026-10-01T08:00:00Z".to_string(),
                kind: "已人工处理".to_string(),
            })
            .expect("备注保存");
        assert_eq!(note.plan_id, plan_id);
        assert_eq!(note.note_id.len(), 64, "备注内容寻址 ID");
        // 同内容同 ID（内容寻址、追加式）
        let note2 = c
            .save_manual_note(&ManualNote {
                plan_id: plan_id.clone(),
                text: "已人工核对流动性，按半量执行".to_string(),
                timestamp: "2026-10-01T08:00:00Z".to_string(),
                kind: "已人工处理".to_string(),
            })
            .expect("重复备注保存");
        assert_eq!(note.note_id, note2.note_id, "同内容备注同 ID");
        // 非法类型拒绝
        let err = c
            .save_manual_note(&ManualNote {
                plan_id,
                text: "x".to_string(),
                timestamp: "2026-10-01T08:00:00Z".to_string(),
                kind: "自动下单".to_string(),
            })
            .expect_err("未知备注类型必须拒绝");
        assert_eq!(err.code, ErrorCode::InvalidArgument);

        // fills 不变
        let fills_after = c
            .query_rows(&run_id, RowsTable::Fills, None, None)
            .expect("备注后 fills 查询");
        let fill_ids_after: Vec<String> = fills_after
            .rows
            .iter()
            .map(|r| match r {
                nautilus_research_domain::protocol::RowsRow::Fill(f) => f.fill_id.clone(),
                other => panic!("应为成交行：{other:?}"),
            })
            .collect();
        assert_eq!(fill_ids_before, fill_ids_after, "备注不得更改回测成交");
    });
}

/// 未实现用例（诚实上报 skip，不可计为通过）：
/// UT-S15-05/06（Windows/Linux 参考机性能量测）、ST-S15-02/03（GUI 双平台旅程）——批次 6。
#[test]
fn deferred_cases_reported_as_skip() {
    for (id, reason) in [
        ("UT-S15-05", "需 Windows 参考机与 GUI（批次 6 性能量测）"),
        ("UT-S15-06", "需 Linux 参考机与 GUI（批次 6 性能量测，不得复用 Windows 结果）"),
        ("ST-S15-02", "需 Windows GUI 双平台旅程（批次 6）"),
        ("ST-S15-03", "需 Linux Wayland/X11 双会话旅程（批次 6）"),
    ] {
        report_result(id, "skip", 0, Some(reason));
    }
}

