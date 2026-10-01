//! S13 研究运行场景测试——批次 3a：信号内核与提交校验。
//!
//! 本批覆盖：UT-S13-01、UT-S13-06、UT-S13-14、UT-S13-15。
//! 其余 UT-S13-02~05/07~13 与 ST-S13-01~03 属批次 3b（真实引擎编排），
//! 本文件先行占位，不代表已实现。

use std::{fs, path::PathBuf, time::Duration};

use nautilus_research_domain::{
    hash::hash_canonical,
    indicators::{ema_last, momentum_score},
    protocol::{
        CostSpec, EffectiveRange, Membership, Rebalance, RunSpec, StrategySpec,
        StrategyTemplate, UniverseMode, UniverseSpec,
    },
    signals::{select_top_k, SignalScore},
    universe::{Condition, RuleChild, RuleField, RuleGroup, RuleOp, AndOp},
    Coordinator, CoordinatorConfig, ErrorCode, ImportSource, ImportSpec, PriceBasis, TaskState,
};
use nautilus_research_testkit::case;

// ------------------------------------------------------------------ 测试夹具

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("research-s13-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn write_file(dir: &PathBuf, name: &str, content: &str) -> String {
    let path = dir.join(name);
    fs::write(&path, content).expect("写入暂存文件");
    path.to_string_lossy().to_string()
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
    let view = c.wait_terminal(&task_ref.task_id, Duration::from_secs(15)).expect("等待导入终态");
    assert_eq!(view.state, TaskState::Succeeded, "导入应成功：{:?}", view.error);
    view.snapshot_id.expect("快照 ID")
}

fn strategy_ema() -> StrategySpec {
    StrategySpec {
        template: StrategyTemplate::Ema,
        version: "1.0.0".to_string(),
        fast: Some(1),
        slow: Some(2),
        lookback: None,
        skip: None,
        top_k: 1,
        rebalance: Rebalance::Daily,
        weight: "equal_slots".to_string(),
        signal_basis: "point_in_time_adjusted".to_string(),
    }
}

fn zero_costs() -> CostSpec {
    CostSpec {
        commission_rate: "0".to_string(),
        min_commission_cny: "0".to_string(),
        sell_tax_rate: "0".to_string(),
        other_fee_rate: "0".to_string(),
        slippage_bps: "0".to_string(),
        participation_rate: "1".to_string(),
        effective_schedule: vec![EffectiveRange {
            start: "2024-01-01".to_string(),
            end: "2024-12-31".to_string(),
        }],
        confirmed: true,
    }
}

fn run_spec() -> RunSpec {
    RunSpec {
        snapshot_id: "snap".to_string(),
        universe_id: "uni".to_string(),
        strategy: strategy_ema(),
        start: "2024-01-02".to_string(),
        end: "2024-01-04".to_string(),
        training_end: None,
        test_start: None,
        capital_cny: "2000".to_string(),
        costs: zero_costs(),
        rules_hash: "a".repeat(64),
        benchmark_snapshot_id: None,
        mode: UniverseMode::Strict,
        grid: None,
        seed: 0,
    }
}

// ------------------------------------------------------------------ UT 用例

#[test]
fn ut_s13_01_ema_and_momentum_values() {
    case("UT-S13-01", || {
        // EMA 快 1 慢 2，收盘 10、11 → 末值 11 与 10.666…
        let closes: Vec<rust_decimal::Decimal> = vec!["10".parse().unwrap(), "11".parse().unwrap()];
        let fast = ema_last(&closes, 1).expect("EMA(1)");
        assert_eq!(fast, "11".parse().unwrap(), "EMA(1) 末值 = 最新收盘");
        let slow = ema_last(&closes, 2).expect("EMA(2)");
        // 10 + (11-10)×2/3 = 32/3
        let expected_slow = rust_decimal::Decimal::from(32u64) / rust_decimal::Decimal::from(3u64);
        assert_eq!(slow, expected_slow, "EMA(2) 末值 = 10.6666…");
        assert!(slow.to_string().starts_with("10.666"), "实际 {slow}");

        // 动量 L=2,s=1，收盘 10、12、15、18 → 15/10-1 = 0.5，不能用 18
        let series: Vec<rust_decimal::Decimal> = ["10", "12", "15", "18"]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect();
        let score = momentum_score(&series, 2, 1).expect("动量分");
        assert_eq!(score, "0.5".parse().unwrap(), "动量 = 15/10-1 = 0.5");
        // 历史不足返回 None，不伪造
        assert_eq!(momentum_score(&series[..2], 2, 1), None);
        assert_eq!(ema_last(&closes[..1], 2), None, "预热不足不产出");
    });
}

#[test]
fn ut_s13_06_submit_validation_rejects_before_enqueue() {
    case("UT-S13-06", || {
        // 样本外日期 ≤ 训练结束 → 拒绝
        let mut spec = run_spec();
        spec.training_end = Some("2024-06-28".to_string());
        spec.test_start = Some("2024-06-28".to_string());
        let err = spec.validate().expect_err("样本外等于训练结束必须拒绝");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("样本外"), "{}", err.message);

        // 网格 >100 组合 → 拒绝
        let mut spec = run_spec();
        let mut grid = std::collections::BTreeMap::new();
        grid.insert(
            "fast".to_string(),
            (1..=11).map(|i| i.to_string()).collect::<Vec<_>>(),
        );
        grid.insert(
            "slow".to_string(),
            (2..=11).map(|i| i.to_string()).collect::<Vec<_>>(),
        );
        spec.grid = Some(grid); // 11×10=110 > 100
        let err = spec.validate().expect_err("网格超限必须拒绝");
        assert!(err.message.contains("100"), "{}", err.message);

        // 网格恰 100 → 通过；成本未确认 → 拒绝
        let mut spec = run_spec();
        let mut grid = std::collections::BTreeMap::new();
        grid.insert("fast".to_string(), (1..=10).map(|i| i.to_string()).collect());
        grid.insert("slow".to_string(), (2..=11).map(|i| i.to_string()).collect());
        spec.grid = Some(grid); // 10×10=100
        spec.validate().expect("100 组合应通过");

        let mut spec = run_spec();
        spec.costs.confirmed = false;
        let err = spec.validate().expect_err("成本未确认必须拒绝");
        assert!(err.message.contains("confirmed"), "{}", err.message);

        // 成本生效区间未覆盖实验 → 拒绝
        let mut spec = run_spec();
        spec.costs.effective_schedule = vec![EffectiveRange {
            start: "2024-02-01".to_string(),
            end: "2024-12-31".to_string(),
        }];
        spec.validate().expect_err("生效区间未覆盖开始日必须拒绝");
    });
}

#[test]
fn ut_s13_14_financial_revision_point_in_time() {
    case("UT-S13-14", || {
        let ws = temp_workspace("ut14");
        let c = Coordinator::open(CoordinatorConfig::new(ws.clone())).expect("打开协调器");
        let master = write_file(&ws, "master.csv", "instrument_id,board,listed_date,delisted_date\nSYN-A,main,2020-01-01,");
        // 快照 1：只有初版报告（roe=5，公告 2024-02-01）
        let fin_v1 = write_file(
            &ws,
            "fin1.csv",
            "instrument_id,period_end,available_at,revision,roe,eps,bps,net_profit,revenue_growth,debt_ratio\n\
             SYN-A,2023-12-31,2024-02-01,0,5,,,,,",
        );
        let snap1 = import_ok(&c, "ut13-14-a", vec![master.clone(), fin_v1]);

        let rule = RuleGroup {
            op: AndOp::And,
            children: vec![RuleChild::Cond(Condition {
                field: RuleField::Roe,
                op: RuleOp::Gte,
                value: Some("6".to_string()),
                values: None,
                window: None,
            })],
        };
        let spec_at = |snap: &str, as_of: &str| UniverseSpec {
            snapshot_id: snap.to_string(),
            as_of: as_of.to_string(),
            mode: UniverseMode::Strict,
            membership: Membership::Fixed,
            rule: rule.clone(),
            missing_policy: None,
        };

        // as_of=2024-03-01：只能读已公告的初版（roe=5 < 6 → 排除）
        let t1 = c.preview_universe("req-ut14-p1", "ut14-p1", spec_at(&snap1, "2024-03-01")).expect("预览1");
        let v1 = c.wait_terminal(&t1.task_id, Duration::from_secs(15)).expect("终态1");
        assert_eq!(v1.state, TaskState::Succeeded);
        let pv1 = c.universe_preview(&t1.task_id).expect("摘要1");
        assert_eq!(pv1.exclude, 1, "初版 roe=5 不满足 ≥6");
        assert_eq!(pv1.pass, 0);

        // 快照 2：追加修订版（roe=9，公告 2024-04-01）→ 新快照、不改旧快照
        let fin_v2 = write_file(
            &ws,
            "fin2.csv",
            "instrument_id,period_end,available_at,revision,roe,eps,bps,net_profit,revenue_growth,debt_ratio\n\
             SYN-A,2023-12-31,2024-02-01,0,5,,,,,\n\
             SYN-A,2023-12-31,2024-04-01,1,9,,,,,",
        );
        let snap2 = import_ok(&c, "ut13-14-b", vec![master, fin_v2]);
        assert_ne!(snap1, snap2, "修订源产生新快照");

        // 新快照 as_of=2024-03-01：修订尚未公告，仍读初版 → 排除
        let t2 = c.preview_universe("req-ut14-p2", "ut14-p2", spec_at(&snap2, "2024-03-01")).expect("预览2");
        c.wait_terminal(&t2.task_id, Duration::from_secs(15)).expect("终态2");
        let pv2 = c.universe_preview(&t2.task_id).expect("摘要2");
        assert_eq!(pv2.exclude, 1, "as_of 前只能读已公告版本");

        // 新快照 as_of=2024-05-01：修订已公告 → roe=9 → 命中
        let t3 = c.preview_universe("req-ut14-p3", "ut14-p3", spec_at(&snap2, "2024-05-01")).expect("预览3");
        c.wait_terminal(&t3.task_id, Duration::from_secs(15)).expect("终态3");
        let pv3 = c.universe_preview(&t3.task_id).expect("摘要3");
        assert_eq!(pv3.pass, 1, "修订公告后读新值");

        // 旧快照不可变：重读 snap1 同样条件仍排除
        let t4 = c.preview_universe("req-ut14-p4", "ut14-p4", spec_at(&snap1, "2024-05-01")).expect("预览4");
        c.wait_terminal(&t4.task_id, Duration::from_secs(15)).expect("终态4");
        let pv4 = c.universe_preview(&t4.task_id).expect("摘要4");
        assert_eq!(pv4.exclude, 1, "新修订不改旧快照内容");

        // 哈希确定性：相同输入哈希 = 相同规范化哈希
        assert_eq!(hash_canonical(&spec_at(&snap1, "2024-03-01")), hash_canonical(&spec_at(&snap1, "2024-03-01")));
    });
}

#[test]
fn ut_s13_15_tie_break_and_empty_slots() {
    case("UT-S13-15", || {
        let s = |id: &str, score: Option<&str>| SignalScore {
            instrument_id: id.to_string(),
            score: score.map(|v| v.parse().unwrap()),
        };
        // 平分稳定排序：B 与 A 同分，按代码升序 A 在前
        let scores = vec![
            s("SYN-B", Some("0.5")),
            s("SYN-A", Some("0.5")),
            s("SYN-C", Some("0.3")),
            s("SYN-D", Some("-0.1")),
            s("SYN-E", None),
        ];
        let picked = select_top_k(&scores, 3);
        assert_eq!(picked, vec!["SYN-A", "SYN-B", "SYN-C"], "平分按代码稳定排序");

        // 正分标的少于 K：未占槽位留现金，不向剩余标的自动加权
        let picked = select_top_k(&scores, 5);
        assert_eq!(picked.len(), 3, "负分与无分不占槽，剩余留现金");

        // 零分不算正分
        let scores = vec![s("SYN-A", Some("0")), s("SYN-B", Some("0.01"))];
        assert_eq!(select_top_k(&scores, 2), vec!["SYN-B"], "零分不占槽");
    });
}
