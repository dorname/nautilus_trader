//! UT-PORT-01/02：组合成交核算（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-PORT-01：OrderFilled 事件驱动开平仓——仓位数量/均价正确，未实现盈亏与
//! 平仓已实现盈亏 = 数量 × 价差，并经组合引擎入账；
//! UT-PORT-02：跨币种核算——BTCUSDT 持仓以 BTC 计量，盈亏换算为结算币 USDT
//! 正确入账（换算与核算正确）。
//!
//! 装配按引擎真实调用链：Position::new → cache.add_position → PositionOpened::
//! create → Portfolio::update_position/update_order（与 execution/engine 的
//! 持仓事件派发路径一致）。

use std::{cell::RefCell, rc::Rc};

use nautilus_common::{cache::Cache, clock::VirtualClock};
use nautilus_core::UUID4;
use nautilus_model::{
    accounts::AccountAny,
    data::QuoteTick,
    enums::{AccountType, OrderSide, OmsType, PositionSide},
    events::{
        AccountState, OrderEventAny, PositionClosed, PositionEvent, PositionOpened,
        order::spec::OrderFilledSpec,
    },
    identifiers::{AccountId, ClientOrderId, PositionId, TradeId},
    instruments::{
        Instrument, InstrumentAny,
        stubs::{audusd_sim, currency_pair_btcusdt},
    },
    position::Position,
    types::{AccountBalance, Currency, Money, Price, Quantity},
};
use nautilus_portfolio::Portfolio;
use nautilus_research_testkit::case;

fn cash_account_state(account_id: &str, usdt_free: &str, usd_free: &str) -> AccountState {
    AccountState::new(
        AccountId::from(account_id),
        AccountType::Cash,
        vec![
            AccountBalance::new(
                Money::from(usdt_free),
                Money::zero(Currency::USDT()),
                Money::from(usdt_free),
            ),
            AccountBalance::new(
                Money::from(usd_free),
                Money::zero(Currency::USD()),
                Money::from(usd_free),
            ),
        ],
        vec![],
        true,
        UUID4::new(),
        nautilus_core::UnixNanos::default(),
        nautilus_core::UnixNanos::default(),
        None,
    )
}

fn make_fill(
    instrument: &InstrumentAny,
    account_id: &str,
    client_order_id: &str,
    trade_id: &str,
    position_id: &str,
    side: OrderSide,
    qty: &str,
    px: &str,
) -> nautilus_model::events::OrderFilled {
    OrderFilledSpec::builder()
        .instrument_id(instrument.id())
        .account_id(AccountId::from(account_id))
        .client_order_id(ClientOrderId::from(client_order_id))
        .trade_id(TradeId::from(trade_id))
        .order_side(side)
        .last_qty(Quantity::from(qty))
        .last_px(Price::from(px))
        .currency(instrument.settlement_currency())
        .position_id(PositionId::from(position_id))
        .build()
}

/// 按引擎调用链应用开仓 fill：建仓入 cache → PositionOpened → 组合事件/余额更新。
/// 返回 (Position, PositionOpened)（事件对象即引擎派发到 events.position.* 的内容）。
fn apply_open_fill(
    portfolio: &mut Portfolio,
    cache: &Rc<RefCell<Cache>>,
    instrument: &InstrumentAny,
    fill: &nautilus_model::events::OrderFilled,
) -> (Position, PositionOpened) {
    let position = Position::new(instrument, fill.clone());
    cache
        .borrow_mut()
        .add_position(&position, OmsType::Hedging)
        .expect("持仓入 cache");
    let opened = PositionOpened::create(
        &position,
        fill,
        UUID4::new(),
        nautilus_core::UnixNanos::default(),
    );
    portfolio.update_position(&PositionEvent::PositionOpened(opened.clone()));
    portfolio.update_order(&OrderEventAny::Filled(fill.clone()));
    (position, opened)
}

/// 按引擎调用链应用平仓 fill：position.apply → cache 更新 → PositionClosed → 组合更新。
fn apply_close_fill(
    portfolio: &mut Portfolio,
    cache: &Rc<RefCell<Cache>>,
    position: &mut Position,
    fill: &nautilus_model::events::OrderFilled,
) -> PositionClosed {
    position.apply(fill);
    cache
        .borrow_mut()
        .update_position(position)
        .expect("平仓更新 cache");
    let closed = PositionClosed::create(
        position,
        fill,
        UUID4::new(),
        nautilus_core::UnixNanos::default(),
    );
    portfolio.update_position(&PositionEvent::PositionClosed(closed.clone()));
    portfolio.update_order(&OrderEventAny::Filled(fill.clone()));
    closed
}

/// UT-PORT-01：OrderFilled 事件驱动开平仓核算（含未实现盈亏增量）。
#[test]
fn ut_port_01_order_filled_position_accounting() {
    case("UT-PORT-01", || {
        let cache = Rc::new(RefCell::new(Cache::new(None, None)));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let instrument = InstrumentAny::CurrencyPair(audusd_sim()); // AUD/USD.SIM，结算 USD
        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("instrument 入 cache");
        let mut portfolio = Portfolio::new(clock, cache.clone(), None);
        let state_01 = cash_account_state("SIM-001", "0 USDT", "10000 USD");
        cache
            .borrow_mut()
            .add_account(AccountAny::Cash(nautilus_model::accounts::CashAccount::new(
                state_01.clone(),
                true,
                false,
            )))
            .expect("账户入 cache");
        portfolio.update_account(&state_01);

        // 开仓：买 100 @1.0
        let fill_open = make_fill(
            &instrument,
            "SIM-001",
            "O-PORT-01-A",
            "T-PORT-01-A",
            "P-PORT-01",
            OrderSide::Buy,
            "100",
            "1.00000",
        );
        let (mut position, opened_event) =
            apply_open_fill(&mut portfolio, &cache, &instrument, &fill_open);
        assert_eq!(
            opened_event.instrument_id, instrument.id(),
            "PositionOpened 事件指向本 instrument"
        );
        assert_eq!(opened_event.quantity, Quantity::from("100"));

        let cached = {
            let cache_ref = cache.borrow();
            cache_ref
                .positions_open(None, None, None, None, None)
                .into_iter()
                .find(|p| p.instrument_id == instrument.id())
                .expect("存在多头持仓")
                .clone()
        };
        assert_eq!(cached.side, PositionSide::Long);
        assert_eq!(cached.quantity, Quantity::from("100"));
        assert_eq!(cached.avg_px_open, 1.0);

        // 未实现盈亏增量：报价 1.10 → (1.10 − 1.00) × 100 = 10 USD
        let quote = QuoteTick::new(
            instrument.id(),
            Price::from("1.10000"),
            Price::from("1.10001"),
            Quantity::from("1"),
            Quantity::from("1"),
            nautilus_core::UnixNanos::default(),
            nautilus_core::UnixNanos::default(),
        );
        cache
            .borrow_mut()
            .add_quote(quote.clone())
            .expect("报价入 cache");
        portfolio.update_quote_tick(&quote);
        let upnl = portfolio
            .unrealized_pnl(&instrument.id())
            .expect("多头持仓必有未实现盈亏");
        assert!(
            (upnl.as_f64() - 10.0).abs() < 0.005,
            "未实现盈亏应为 10 USD，实测 {upnl}"
        );

        // 平仓：卖 100 @1.2 → 已实现盈亏 = 100 × (1.2 − 1.0) = 20 USD
        let fill_close = make_fill(
            &instrument,
            "SIM-001",
            "O-PORT-01-B",
            "T-PORT-01-B",
            "P-PORT-01",
            OrderSide::Sell,
            "100",
            "1.20000",
        );
        let closed = apply_close_fill(&mut portfolio, &cache, &mut position, &fill_close);

        let realized = closed.realized_pnl.expect("平仓事件必含已实现盈亏");
        assert_eq!(realized.currency, Currency::USD());
        assert!(
            (realized.as_f64() - 20.0).abs() < 0.005,
            "已实现盈亏 = 100 × (1.2 − 1.0) = 20 USD，实测 {realized}"
        );
        assert!(
            cache
                .borrow()
                .positions_open(None, None, None, None, None)
                .is_empty(),
            "平仓后无未平持仓"
        );

        // 已实现盈亏入账：USD 余额 10000 + 20 = 10020
        let usd_total = {
            let cache_ref = cache.borrow();
            let account = cache_ref
                .account_owned(&AccountId::from("SIM-001"))
                .expect("账户在 cache");
            account
                .balances()
                .get(&Currency::USD())
                .expect("USD 余额")
                .total
                .as_f64()
        };
        assert!(
            (usd_total - 10_020.0).abs() < 0.005,
            "USD 余额应入账为 10020，实测 {usd_total}"
        );
    });
}

/// UT-PORT-02：跨币种换算核算——BTC 持仓盈亏换算为结算币 USDT 入账。
#[test]
fn ut_port_02_cross_currency_accounting() {
    case("UT-PORT-02", || {
        let cache = Rc::new(RefCell::new(Cache::new(None, None)));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let instrument = InstrumentAny::CurrencyPair(currency_pair_btcusdt()); // BTCUSDT.BINANCE
        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("instrument 入 cache");
        let mut portfolio = Portfolio::new(clock, cache.clone(), None);
        let state_02 = cash_account_state("BINANCE-001", "100000 USDT", "0 USD");
        cache
            .borrow_mut()
            .add_account(AccountAny::Cash(nautilus_model::accounts::CashAccount::new(
                state_02.clone(),
                true,
                false,
            )))
            .expect("账户入 cache");
        portfolio.update_account(&state_02);

        // 开仓：买 1 BTC @50000 USDT（持仓以基础货币 BTC 计量）
        let fill_open = make_fill(
            &instrument,
            "BINANCE-001",
            "O-PORT-02-A",
            "T-PORT-02-A",
            "P-PORT-02",
            OrderSide::Buy,
            "1.000000",
            "50000.00",
        );
        let (mut position, _opened) = apply_open_fill(&mut portfolio, &cache, &instrument, &fill_open);

        let cached = {
            let cache_ref = cache.borrow();
            cache_ref
                .positions_open(None, None, None, None, None)
                .into_iter()
                .find(|p| p.instrument_id == instrument.id())
                .expect("存在 BTC 多头持仓")
                .clone()
        };
        assert_eq!(cached.side, PositionSide::Long);
        assert_eq!(cached.quantity, Quantity::from("1.000000"), "持仓以 BTC 计量");
        assert_eq!(cached.avg_px_open, 50000.0);
        assert_eq!(cached.settlement_currency, Currency::USDT());

        // 平仓：卖 1 BTC @52000 → 已实现盈亏 = 1 × (52000 − 50000) = 2000 USDT
        let fill_close = make_fill(
            &instrument,
            "BINANCE-001",
            "O-PORT-02-B",
            "T-PORT-02-B",
            "P-PORT-02",
            OrderSide::Sell,
            "1.000000",
            "52000.00",
        );
        let closed = apply_close_fill(&mut portfolio, &cache, &mut position, &fill_close);

        let realized = closed.realized_pnl.expect("平仓事件必含已实现盈亏");
        assert_eq!(realized.currency, Currency::USDT(), "盈亏以结算币 USDT 换算");
        assert!(
            (realized.as_f64() - 2000.0).abs() < 0.005,
            "已实现盈亏 = 1 × (52000 − 50000) = 2000 USDT，实测 {realized}"
        );

        // 跨币种核算：USDT 余额 100000 + 2000 = 102000
        let usdt_total = {
            let cache_ref = cache.borrow();
            let account = cache_ref
                .account_owned(&AccountId::from("BINANCE-001"))
                .expect("账户在 cache");
            account
                .balances()
                .get(&Currency::USDT())
                .expect("USDT 余额")
                .total
                .as_f64()
        };
        assert!(
            (usdt_total - 102_000.0).abs() < 0.005,
            "USDT 余额应入账为 102000，实测 {usdt_total}"
        );
    });
}
