//! S13 公司行为 / 日历调仓 / 规则包估值测试——批次 3d
//! （规格：logos/resources/test/core-S13-test-cases.md）。
//!
//! 本批覆盖：UT-S13-08、UT-S13-10、UT-S13-11、UT-S13-12。
//! 除权除息与调仓门控以真实引擎编排数值断言；规则包生效区间为纯函数断言。

use std::collections::BTreeSet;

use nautilus_research_domain::{
    auxiliary::{
        actions_from_bytes, actions_json_bytes, calendar_from_bytes, calendar_json_bytes,
        parse_rules_csv, rules_from_bytes, rules_json_bytes, ActionRecord, CalendarRecord,
        RuleRecord,
    },
    corporate,
};
use nautilus_research_testkit::case;
use nautilus_research_worker::{
    gate::BoardRules,
    runner::{run_ema_daily, DayBar, EmaRunConfig},
};
use rust_decimal::Decimal;

// ------------------------------------------------------------------ 夹具

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

/// 追加一日行情（全字段同价或分别给出）。
fn bar(date: &str, open: &str, close: &str) -> DayBar {
    let (o, c): (Decimal, Decimal) = (open.parse().unwrap(), close.parse().unwrap());
    let high = o.max(c);
    let low = o.min(c);
    DayBar {
        date: date.to_string(),
        open: o,
        high,
        low,
        close: c,
        volume: 100_000,
    }
}

fn rules() -> BoardRules {
    BoardRules {
        board: "synthetic".to_string(),
        tick: "0.01".parse().unwrap(),
        min_qty: 100,
        qty_step: 100,
        limit_pct: None,
    }
}

fn config(capital: i64, actions: Vec<ActionRecord>) -> EmaRunConfig {
    EmaRunConfig {
        fast: 1,
        slow: 2,
        warmup_bars: 20,
        capital: Decimal::from(capital),
        commission_rate: Decimal::ZERO,
        min_commission: Decimal::ZERO,
        sell_tax_rate: Decimal::ZERO,
        other_fee_rate: Decimal::ZERO,
        rules: rules(),
        signal_dates: None,
        actions,
    }
}

fn equity_of(outcome: &nautilus_research_worker::runner::RunOutcome, date: &str) -> nautilus_research_worker::runner::EquityPoint {
    outcome
        .equity_curve
        .iter()
        .find(|p| p.date == date)
        .unwrap_or_else(|| panic!("缺少 {date} 估值点"))
        .clone()
}

// ------------------------------------------------------------------ 用例

/// UT-S13-08：拆股 2 倍——前一日持 100 股价格 20（此处 12 同比例），
/// 除权日价格减半；持仓翻倍、净值不凭空减半；事件仅按可见生效日应用。
#[test]
fn ut_s13_08_split_doubles_holdings_preserves_equity() {
    case("UT-S13-08", || {
        let action = ActionRecord {
            instrument_id: "TEST.SH".to_string(),
            announced_at: "2024-01-04".to_string(), // 除权日前公告 → 除权日可见
            ex_date: "2024-01-05".to_string(),
            pay_date: None,
            split_ratio: Some(Decimal::from(2)),
            cash_per_share: None,
        };
        let mut bars = warmup_bars();
        bars.push(bar("2024-01-02", "10", "10"));
        bars.push(bar("2024-01-03", "10", "11")); // 金叉信号
        bars.push(bar("2024-01-04", "12", "12")); // 次日开盘 12 买入 100 股
        bars.push(bar("2024-01-05", "6", "6")); // 除权日：价格减半
        bars.push(bar("2024-01-08", "6.5", "6.5"));
        let outcome = run_ema_daily("TEST.SH", "SSE", &bars, &config(2000, vec![action.clone()]))
            .expect("引擎运行");

        // 买入：2024-01-04 成交 100 股 @ 12，现金 800、净值 2000
        assert_eq!(outcome.fills[0].side, "buy");
        assert_eq!((outcome.fills[0].date.as_str(), outcome.fills[0].qty, outcome.fills[0].price.as_str()), ("2024-01-04", 100, "12"));
        let e4 = equity_of(&outcome, "2024-01-04");
        assert_eq!((e4.position_qty, e4.equity_cny.as_str()), (100, "2000"));

        // 除权日：持仓翻倍 200 股、净值 800+200×6=2000 不凭空减半
        let e5 = equity_of(&outcome, "2024-01-05");
        assert_eq!((e5.position_qty, e5.equity_cny.as_str()), (200, "2000"));

        // 卖出信号形成于除权日收盘：跌势触发、按引擎持仓 100 股封顶成交
        // （引擎 CASH 账户禁止卖空且无持仓调整接口——拆股不改变引擎内持仓，
        // 溢余 100 股在估值层保留；v1 已知限制，见实现清单批次 3d）
        let sell = outcome.fills.iter().find(|f| f.side == "sell").expect("应有卖出");
        assert_eq!((sell.date.as_str(), sell.qty, sell.price.as_str()), ("2024-01-08", 100, "6.5"));
        // 估值层：卖出后仍持 100 股（200-100），现金 800+650=1450、净值 2100
        let e8 = equity_of(&outcome, "2024-01-08");
        assert_eq!((e8.position_qty, e8.equity_cny.as_str()), (100, "2100"));

        // 时点纪律：公告晚于除权日 → 事件不可见、不应用
        let late = ActionRecord { announced_at: "2024-01-06".to_string(), ..action.clone() };
        assert!(corporate::apply_action(100, &late, "2024-01-05").is_none(), "未公告事件不得应用");
        assert!(corporate::apply_action(100, &action, "2024-01-05").is_some(), "已公告事件在除权日应用");

        // 数据通道：规范化字节往返一致
        let round = actions_from_bytes(&actions_json_bytes(&[action])).expect("规范化解析");
        assert_eq!(round.len(), 1);
    });
}

/// UT-S13-11：股数 100、每股现金分红 0.1、除息日与支付日不同——
/// 除息日应收 10（计入总资产一次）、支付日应收转现金 10，总资产不重复增加。
#[test]
fn ut_s13_11_dividend_receivable_then_cash_once() {
    case("UT-S13-11", || {
        let action = ActionRecord {
            instrument_id: "TEST.SH".to_string(),
            announced_at: "2024-01-04".to_string(),
            ex_date: "2024-01-05".to_string(),
            pay_date: Some("2024-01-08".to_string()),
            split_ratio: None,
            cash_per_share: Some("0.1".parse().unwrap()),
        };
        let mut bars = warmup_bars();
        bars.push(bar("2024-01-02", "10", "10"));
        bars.push(bar("2024-01-03", "10", "11")); // 金叉信号
        bars.push(bar("2024-01-04", "12", "12")); // 买入 100 股 @ 12
        bars.push(bar("2024-01-05", "12", "12")); // 除息日
        bars.push(bar("2024-01-08", "12", "12")); // 支付日
        let outcome = run_ema_daily("TEST.SH", "SSE", &bars, &config(2000, vec![action]))
            .expect("引擎运行");

        // 除息前：现金 800、净值 2000
        let e4 = equity_of(&outcome, "2024-01-04");
        assert_eq!((e4.position_qty, e4.cash_cny.as_str(), e4.equity_cny.as_str()), (100, "800", "2000"));

        // 除息日：应收 100×0.1=10 计入总资产（现金未变）
        let e5 = equity_of(&outcome, "2024-01-05");
        assert_eq!((e5.position_qty, e5.cash_cny.as_str(), e5.equity_cny.as_str()), (100, "800", "2010"));

        // 支付日：应收转现金 10，总资产仍 2010（不重复增加）
        let e8 = equity_of(&outcome, "2024-01-08");
        assert_eq!((e8.position_qty, e8.cash_cny.as_str(), e8.equity_cny.as_str()), (100, "810", "2010"));
        assert_eq!(e8.equity_cny, e5.equity_cny, "支付日总资产与除息日一致（只计一次）");

        // 无重复计息：支付日后应收清零（经由末值等于现金+持仓市值验证）
        let last = outcome.equity_curve.last().unwrap();
        let cash: Decimal = last.cash_cny.parse().unwrap();
        let market: Decimal = Decimal::from(last.position_qty) * last.close.parse::<Decimal>().unwrap();
        assert_eq!(cash + market, last.equity_cny.parse::<Decimal>().unwrap(), "净值=现金+市值，应收不残留");
    });
}

/// UT-S13-10：节假日周末按交易日历重平衡——weekly/monthly 只在周期最后
/// 交易日形成信号，下一交易日执行（跨周末顺延）。
#[test]
fn ut_s13_10_calendar_rebalance_signal_on_period_end() {
    case("UT-S13-10", || {
        let calendar: Vec<CalendarRecord> = [
            "2024-01-08", "2024-01-09", "2024-01-10", "2024-01-11", "2024-01-12",
            "2024-01-15", "2024-01-16", "2024-01-17", "2024-01-18", "2024-01-19",
            "2024-01-22",
        ]
        .iter()
        .map(|d| CalendarRecord { trade_date: d.to_string() })
        .collect();

        // 信号日：每周最后一个交易日；执行日为下一交易日
        let signals = corporate::rebalance_signal_dates(
            &calendar,
            nautilus_research_domain::protocol::Rebalance::Weekly,
            "2024-01-08",
            "2024-01-22",
        );
        assert_eq!(
            signals,
            vec!["2024-01-12".to_string(), "2024-01-19".to_string(), "2024-01-22".to_string()]
        );
        let monthly = corporate::rebalance_signal_dates(
            &calendar,
            nautilus_research_domain::protocol::Rebalance::Monthly,
            "2024-01-08",
            "2024-01-22",
        );
        assert_eq!(monthly, vec!["2024-01-22".to_string()], "月末调仓只在最后一个交易日");
        assert_eq!(corporate::next_trade_date(&calendar, "2024-01-12").as_deref(), Some("2024-01-15"), "跨周末顺延");
        assert_eq!(corporate::next_trade_date(&calendar, "2024-01-22"), None, "区间末无下一交易日不执行");

        // 数据通道：日历规范化字节往返一致
        let round = calendar_from_bytes(&calendar_json_bytes(&calendar)).expect("规范化解析");
        assert_eq!(round.len(), calendar.len());

        // 引擎级：只在信号日形成信号、下一交易日成交（1/12 信号 → 1/15 开盘成交）
        let mut bars: Vec<DayBar> = Vec::new();
        let closes = [
            ("2024-01-08", "10"), ("2024-01-09", "11"), ("2024-01-10", "12"),
            ("2024-01-11", "13"), ("2024-01-12", "14"),
            ("2024-01-15", "15"), ("2024-01-16", "16"), ("2024-01-17", "17"),
            ("2024-01-18", "18"), ("2024-01-19", "17"),
            ("2024-01-22", "16"),
        ];
        for (i, (date, close)) in closes.iter().enumerate() {
            // 执行日开盘价 = 前收（信号日收盘价，无隔日跳空，现金估算与成交一致）
            let prev_close = if i > 0 { closes[i - 1].1 } else { close };
            bars.push(bar(date, prev_close, close));
        }
        let cfg = EmaRunConfig {
            fast: 1,
            slow: 2,
            warmup_bars: 0,
            capital: Decimal::from(20000),
            commission_rate: Decimal::ZERO,
            min_commission: Decimal::ZERO,
            sell_tax_rate: Decimal::ZERO,
            other_fee_rate: Decimal::ZERO,
            rules: rules(),
            signal_dates: Some(signals.iter().cloned().collect::<BTreeSet<String>>()),
            actions: Vec::new(),
        };
        let outcome = run_ema_daily("TEST.SH", "SSE", &bars, &cfg).expect("引擎运行");

        // 每日调仓会在 1/10 之前买入；门控后首笔成交在 1/15（1/12 信号）
        assert!(outcome.fills.iter().all(|f| f.date != "2024-01-09" && f.date != "2024-01-10"), "非执行日不得成交");
        assert_eq!(outcome.fills.len(), 2, "两次调仓各一笔");
        let buy = &outcome.fills[0];
        assert_eq!((buy.date.as_str(), buy.side.as_str(), buy.qty, buy.price.as_str()), ("2024-01-15", "buy", 1400, "14"));
        let sell = &outcome.fills[1];
        assert_eq!((sell.date.as_str(), sell.side.as_str(), sell.qty, sell.price.as_str()), ("2024-01-22", "sell", 1400, "17"));
    });
}

/// UT-S13-12：规则包缺历史生效区间——严格任务失败并定位板块／日期，
/// 不以 0 或最新价（现行规则）静默代替。
#[test]
fn ut_s13_12_rules_pack_gap_fails_strict_with_location() {
    case("UT-S13-12", || {
        let csv = "market,board,effective_start,effective_end,tick,min_qty,qty_step,limit_pct\n\
                   SSE,主板,2024-01-01,2024-06-30,0.01,100,100,0.1\n\
                   SSE,主板,2024-06-30,,0.01,200,100,0.2\n";
        let pack: Vec<RuleRecord> = parse_rules_csv(csv).expect("规则包解析");

        // 前闭后开：切换日 6/30 起用新规则；开放区间末端有效
        let r = corporate::resolve_rules(&pack, "主板", "2024-06-29").expect("旧区间生效");
        assert_eq!(r.min_qty, 100);
        let r = corporate::resolve_rules(&pack, "主板", "2024-06-30").expect("切换日起新规则");
        assert_eq!(r.min_qty, 200);
        let r = corporate::resolve_rules(&pack, "主板", "2025-01-02").expect("开放区间现行有效");
        assert_eq!(r.min_qty, 200);

        // 缺口：2023-12-31 早于首个生效区间 → 严格失败并定位板块/日期
        let err = corporate::resolve_rules(&pack, "主板", "2023-12-31").expect_err("缺区间必须失败");
        let msg = err.to_string();
        assert!(msg.contains("主板") && msg.contains("2023-12-31"), "错误须定位板块与日期：{msg}");
        assert!(msg.contains("不得"), "错误须声明不得以现行规则反套历史：{msg}");

        // 缺板块：创业板无任何生效区间 → 定位板块
        let err = corporate::resolve_rules(&pack, "创业板", "2024-07-01").expect_err("未知板块失败");
        assert!(err.to_string().contains("创业板"));

        // 区间覆盖完整性：返回区间内第一个缺失日
        let calendar: Vec<CalendarRecord> = ["2023-12-29", "2024-01-02", "2024-01-03"]
            .iter()
            .map(|d| CalendarRecord { trade_date: d.to_string() })
            .collect();
        let missing = corporate::first_missing_rule_date(&pack, "主板", &calendar, "2023-12-29", "2024-01-03");
        assert_eq!(missing.as_deref(), Some("2023-12-29"));
        let covered: Vec<CalendarRecord> = ["2024-01-02", "2024-01-03"]
            .iter()
            .map(|d| CalendarRecord { trade_date: d.to_string() })
            .collect();
        assert_eq!(
            corporate::first_missing_rule_date(&pack, "主板", &covered, "2024-01-02", "2024-01-03"),
            None,
            "覆盖区间内无缺失"
        );

        // 数据通道：规则包规范化字节往返一致（按 (market,board,start) 排序）
        let round = rules_from_bytes(&rules_json_bytes(&pack)).expect("规范化解析");
        assert_eq!(round.len(), pack.len());
    });
}
