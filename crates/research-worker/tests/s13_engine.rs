//! S13 引擎运行测试——批次 3b（规格：logos/resources/test/core-S13-test-cases.md）。
//!
//! 本批覆盖：UT-S13-02、UT-S13-03、UT-S13-04、UT-S13-05、UT-S13-09、UT-S13-13、ST-S13-01。
//! ST-S13-01 使用真实 Nautilus 引擎编排（BacktestEngine + bar_execution），
//! 隔日执行由数值断言证明：信号日收盘 11 不成交，次日开盘 12 成交。

use nautilus_research_domain::hash::canonical_json;
use nautilus_research_testkit::case;
use nautilus_research_worker::{
    fees::astock_fee,
    gate::{gate_open_price, size_buy, validate_buy_qty, validate_sell_qty, BoardRules, Reject},
    runner::{run_ema_daily, DayBar, EmaRunConfig, RunOutcome},
};
use rust_decimal::Decimal;

// ------------------------------------------------------------------ F-RUN 夹具

/// 20 个预热交易日（2023-12-04～2023-12-29 工作日）收盘均 10，量 100000。
fn warmup_bars() -> Vec<DayBar> {
    let dates = [
        "2023-12-04", "2023-12-05", "2023-12-06", "2023-12-07", "2023-12-08",
        "2023-12-11", "2023-12-12", "2023-12-13", "2023-12-14", "2023-12-15",
        "2023-12-18", "2023-12-19", "2023-12-20", "2023-12-21", "2023-12-22",
        "2023-12-25", "2023-12-26", "2023-12-27", "2023-12-28", "2023-12-29",
    ];
    dates
        .iter()
        .map(|d| DayBar {
            date: d.to_string(),
            open: Decimal::from(10),
            high: Decimal::from(10),
            low: Decimal::from(10),
            close: Decimal::from(10),
            volume: 100_000,
        })
        .collect()
}

/// F-RUN 完整行情：预热 20 日 + 正式区间 2024-01-02～04。
fn frun_bars() -> Vec<DayBar> {
    let mut bars = warmup_bars();
    let d = |date: &str, o: &str, h: &str, l: &str, c: &str| DayBar {
        date: date.to_string(),
        open: o.parse().unwrap(),
        high: h.parse().unwrap(),
        low: l.parse().unwrap(),
        close: c.parse().unwrap(),
        volume: 100_000,
    };
    bars.push(d("2024-01-02", "10", "10", "10", "10"));
    bars.push(d("2024-01-03", "10", "11", "10", "11"));
    bars.push(d("2024-01-04", "12", "13", "12", "13"));
    bars
}

fn frun_rules() -> BoardRules {
    BoardRules {
        board: "synthetic".to_string(),
        tick: "0.01".parse().unwrap(),
        min_qty: 100,
        qty_step: 100,
        limit_pct: None,
    }
}

fn frun_config(commission_rate: &str, min_commission: &str) -> EmaRunConfig {
    EmaRunConfig {
        fast: 1,
        slow: 2,
        warmup_bars: 20,
        capital: Decimal::from(2000),
        commission_rate: commission_rate.parse().unwrap(),
        min_commission: min_commission.parse().unwrap(),
        sell_tax_rate: Decimal::ZERO,
        other_fee_rate: Decimal::ZERO,
        rules: frun_rules(),
    }
}

fn run_frun(config: &EmaRunConfig) -> RunOutcome {
    run_ema_daily("TEST.SH", "SSE", &frun_bars(), config).expect("F-RUN 引擎运行")
}

// ------------------------------------------------------------------ 用例

#[test]
fn ut_s13_02_next_day_open_fill_not_same_bar() {
    case("UT-S13-02", || {
        let outcome = run_frun(&frun_config("0", "0"));
        // 目标 100 股于次日（2024-01-04）开盘 12 成交
        assert_eq!(outcome.fills.len(), 1, "只有一笔买入成交");
        let fill = &outcome.fills[0];
        assert_eq!(fill.date, "2024-01-04", "次日成交");
        assert_eq!(fill.side, "buy");
        assert_eq!(fill.qty, 100);
        assert_eq!(fill.price, "12", "开盘 12 成交");
        // 信号日（1/3）收盘 11 不得成交（禁止同 bar 成交）
        assert!(
            outcome.fills.iter().all(|f| f.date != "2024-01-03"),
            "信号日不得成交"
        );
        assert!(
            outcome.fills.iter().all(|f| f.price != "11"),
            "不得以信号日收盘价成交"
        );
        assert_eq!(outcome.final_cash_cny, "800", "现金 2000-1200=800");
    });
}

#[test]
fn ut_s13_03_commission_min_fee() {
    case("UT-S13-03", || {
        // 纯函数：成交额 1200、佣金率 0.001、最低 5 → 佣金 5（非 1.2）
        let fee = astock_fee(
            false,
            Decimal::from(1200),
            "0.001".parse().unwrap(),
            Decimal::from(5),
            Decimal::ZERO,
            Decimal::ZERO,
        );
        assert_eq!(fee, Decimal::from(5), "最低佣金兜底");

        // 引擎级：同一成交经 A 股费用模型扣费，现金 2000-1200-5=795
        let outcome = run_frun(&frun_config("0.001", "5"));
        assert_eq!(outcome.fills.len(), 1);
        assert_eq!(outcome.fills[0].fee_cny, "5", "引擎佣金 = 5");
        assert_eq!(outcome.final_cash_cny, "795", "现金 795");
    });
}

#[test]
fn ut_s13_04_gate_rejections_with_reasons() {
    case("UT-S13-04", || {
        let rules = BoardRules {
            limit_pct: Some("0.1".parse().unwrap()),
            ..frun_rules()
        };
        // 停牌
        let err = gate_open_price(false, "2024-01-04", true, Some("12".parse().unwrap()), Some("11".parse().unwrap()), &rules)
            .expect_err("停牌拒绝");
        assert!(matches!(err, Reject::Suspended { .. }), "{err:?}");
        // 缺价格
        let err = gate_open_price(false, "2024-01-04", false, None, Some("11".parse().unwrap()), &rules)
            .expect_err("缺价拒绝");
        assert!(matches!(err, Reject::PriceMissing { .. }));
        // 开盘涨停买入拒绝（昨收 11，涨停 12.1，开盘 12.1）
        let err = gate_open_price(false, "2024-01-04", false, Some("12.1".parse().unwrap()), Some("11".parse().unwrap()), &rules)
            .expect_err("涨停买入拒绝");
        assert!(matches!(err, Reject::LimitUp { .. }));
        // 开盘跌停卖出拒绝（昨收 11，跌停 9.9）
        let err = gate_open_price(true, "2024-01-04", false, Some("9.9".parse().unwrap()), Some("11".parse().unwrap()), &rules)
            .expect_err("跌停卖出拒绝");
        assert!(matches!(err, Reject::LimitDown { .. }));
        // T+1：当日新买 100 立刻卖 → 拒绝；不产生允许的同日卖出
        let err = validate_sell_qty(100, 0, &frun_rules()).expect_err("T+1 拒绝");
        assert!(matches!(err, Reject::NotSellableT1 { requested: 100, sellable: 0 }));
        assert!(err.describe().contains("T+1"), "理由需说明 T+1");
        // 昨日买入今日可卖
        validate_sell_qty(100, 100, &frun_rules()).expect("隔日可卖");
    });
}

#[test]
fn ut_s13_05_deterministic_rerun_identical() {
    case("UT-S13-05", || {
        let a = run_frun(&frun_config("0", "0"));
        let b = run_frun(&frun_config("0", "0"));
        // 规范化结果一致：run_id 与墙钟时间不进入计算内容
        let ja = canonical_json(&a);
        let jb = canonical_json(&b);
        assert_eq!(ja, jb, "同配置同快照两次运行规范化结果必须一致");
        assert_eq!(a.fills, b.fills);
        assert_eq!(a.equity_curve, b.equity_curve);
    });
}

#[test]
fn ut_s13_09_board_specific_lot_rules() {
    case("UT-S13-09", || {
        // 合成板块规则：最小 200、步长 1 → 199 拒绝、201 合法；不套用全市场 100 股
        let board_rules = BoardRules {
            board: "synthetic-gem".to_string(),
            tick: "0.01".parse().unwrap(),
            min_qty: 200,
            qty_step: 1,
            limit_pct: None,
        };
        let err = validate_buy_qty(199, &board_rules).expect_err("低于板块最小量拒绝");
        assert!(matches!(err, Reject::BelowMinQty { qty: 199, min: 200 }));
        validate_buy_qty(201, &board_rules).expect("201 合法");
        // 默认主板规则：步长 100 → 150 拒绝
        let err = validate_buy_qty(150, &frun_rules()).expect_err("步长不符拒绝");
        assert!(matches!(err, Reject::BadStep { qty: 150, step: 100 }));
    });
}

#[test]
fn ut_s13_13_cash_aware_shrink_never_negative() {
    case("UT-S13-13", || {
        let rules = frun_rules();
        let zero_fee = |q: u64| Decimal::from(q) * Decimal::from(12);
        // 现金 2000、估 12：现金上限 166 → 步长 100 → 100 股
        assert_eq!(size_buy(Decimal::from(2000), Decimal::from(12), u64::MAX / 2, &rules, zero_fee), 100);
        // 含费逐笔扣：佣金最低 5 → 100 股成本 1205；现金 1204 时缩到 0（现金不负）
        let fee5 = |q: u64| {
            let t = Decimal::from(q) * Decimal::from(12);
            t + astock_fee(false, t, "0.001".parse().unwrap(), Decimal::from(5), Decimal::ZERO, Decimal::ZERO)
        };
        assert_eq!(size_buy(Decimal::from(2000), Decimal::from(12), u64::MAX / 2, &rules, fee5), 100);
        assert_eq!(size_buy(Decimal::from(1204), Decimal::from(12), u64::MAX / 2, &rules, fee5), 0, "含费超现金缩到 0");
        // 低于最小量不买
        assert_eq!(size_buy(Decimal::from(500), Decimal::from(12), u64::MAX / 2, &rules, zero_fee), 0);
    });
}

#[test]
fn st_s13_01_frun_real_engine_orchestration() {
    case("ST-S13-01", || {
        let outcome = run_frun(&frun_config("0", "0"));
        // 唯一成交日 2024-01-04，100 股价格 12
        assert_eq!(outcome.fills.len(), 1, "唯一一笔成交");
        let fill = &outcome.fills[0];
        assert_eq!((fill.date.as_str(), fill.qty, fill.price.as_str()), ("2024-01-04", 100, "12"));
        // 现金 800、末净值 2100、总收益 0.05
        assert_eq!(outcome.final_cash_cny, "800");
        assert_eq!(outcome.final_equity_cny, "2100");
        assert_eq!(outcome.total_return, "0.05");
        // 期末不强平：持仓 100 股到期末
        let last = outcome.equity_curve.last().expect("末日估值");
        assert_eq!(last.position_qty, 100);
        assert_eq!(last.close, "13");
        // 预热不交易：前 20 日无成交
        assert!(outcome.fills.iter().all(|f| f.date.as_str() >= "2024-01-02"));
        // 隔日执行：信号形成日 1/3 无成交
        assert!(outcome.fills.iter().all(|f| f.date != "2024-01-03"));
    });
}
