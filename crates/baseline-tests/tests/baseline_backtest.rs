//! UT-BT-01/02：回测引擎（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-BT-02：撮合仿真——市价/限价单进入 L1 订单簿，成交价与数量正确；
//! UT-BT-01：回测确定性——同一输入序列两次运行，事件流（语义字段）逐一致。

use std::{cell::RefCell, rc::Rc};

use nautilus_backtest::{
    config::SimulatedVenueConfig,
    exchange::SimulatedExchange,
    execution_client::BacktestExecutionClient,
};
use nautilus_common::{
    cache::Cache,
    clock::VirtualClock,
    messages::execution::{SubmitOrder, TradingCommand},
    msgbus::{self, MessagingSwitchboard, stubs::get_typed_into_message_saving_handler},
};
use nautilus_core::{UUID4, UnixNanos};
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use nautilus_model::{
    enums::{AccountType, BookType, LiquiditySide, OmsType, OrderSide, OrderType},
    events::OrderEventAny,
    identifiers::{AccountId, ClientOrderId, TraderId, Venue},
    instruments::{CryptoPerpetual, InstrumentAny, stubs::crypto_perpetual_ethusdt},
    orders::{Order, OrderAny, OrderTestBuilder, stubs::TestOrderEventStubs},
    types::{Currency, Money, Price, Quantity},
};
use nautilus_research_testkit::case;
use rust_decimal::Decimal;

/// 构建单个模拟交易所（含回测执行客户端），返回 (exchange, 事件捕获)。
fn build_exchange(
    cache: Rc<RefCell<Cache>>,
) -> (
    Rc<RefCell<SimulatedExchange>>,
    nautilus_common::msgbus::stubs::TypedIntoMessageSavingHandler<OrderEventAny>,
) {
    *msgbus::get_message_bus().borrow_mut() = msgbus::MessageBus::default();

    let (event_handler, event_saved) = get_typed_into_message_saving_handler::<OrderEventAny>(None);
    msgbus::register_order_event_endpoint(MessagingSwitchboard::exec_engine_process(), event_handler);

    let clock = Rc::new(RefCell::new(VirtualClock::new()));
    let config = SimulatedVenueConfig::builder()
        .venue(Venue::from("BINANCE"))
        .oms_type(OmsType::Netting)
        .account_type(AccountType::Margin)
        .book_type(BookType::L1_MBP)
        .starting_balances(vec![Money::new(1000.0, Currency::USD())])
        .default_leverage(Decimal::ONE)
        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()).into())
        .build()
        .expect("场所配置合法");
    let exchange = Rc::new(RefCell::new(
        SimulatedExchange::new(config, cache.clone(), clock.clone()).expect("交易所构建"),
    ));
    SimulatedExchange::register_spread_quote_endpoint(&exchange);

    let client_clock = Rc::new(RefCell::new(VirtualClock::new()));
    let execution_client = BacktestExecutionClient::new(
        TraderId::from("TRADER-001"),
        AccountId::from("BINANCE-001"),
        &exchange,
        cache,
        client_clock,
        None,
        None,
    );
    exchange
        .borrow_mut()
        .register_client(Rc::new(execution_client));

    (exchange, event_saved)
}

fn eth_perp() -> CryptoPerpetual {
    crypto_perpetual_ethusdt()
}

fn quote_for(instrument: &CryptoPerpetual, bid: &str, ask: &str, ts: u64) -> nautilus_model::data::QuoteTick {
    nautilus_model::data::QuoteTick::new(
        instrument.id,
        Price::from(bid),
        Price::from(ask),
        Quantity::from("1.000"),
        Quantity::from("1.000"),
        UnixNanos::from(ts),
        UnixNanos::from(ts),
    )
}

fn limit_order(instrument: &CryptoPerpetual, client_order_id: &str, side: OrderSide, price: &str) -> OrderAny {
    OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(instrument.id)
        .client_order_id(ClientOrderId::from(client_order_id))
        .side(side)
        .quantity(Quantity::from("1.000"))
        .price(Price::from(price))
        .build()
}

fn market_order(instrument: &CryptoPerpetual, client_order_id: &str, side: OrderSide) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument.id)
        .client_order_id(ClientOrderId::from(client_order_id))
        .side(side)
        .quantity(Quantity::from("1.000"))
        .build()
}

fn submit(exchange: &Rc<RefCell<SimulatedExchange>>, cache: &Rc<RefCell<Cache>>, order: &OrderAny, ts: u64) {
    cache
        .borrow_mut()
        .add_order(order.clone(), None, None, false)
        .expect("订单入 cache");
    cache
        .borrow_mut()
        .update_order(&TestOrderEventStubs::submitted(
            order,
            AccountId::from("BINANCE-001"),
        ))
        .expect("订单状态更新");
    let command = TradingCommand::SubmitOrder(SubmitOrder::new(
        TraderId::from("TRADER-001"),
        None,
        order.strategy_id(),
        order.instrument_id(),
        order.client_order_id(),
        order.init_event().clone(),
        None,
        None,
        None,
        UUID4::new(),
        UnixNanos::from(ts),
        None,
    ));
    exchange.borrow_mut().send(command);
    exchange.borrow_mut().process(UnixNanos::from(ts));
}

/// 事件流语义摘要：仅含可跨次比对的确定性字段（不含事件 UUID/时间戳）。
fn event_digest(events: &[OrderEventAny]) -> Vec<String> {
    events
        .iter()
        .map(|event| match event {
            OrderEventAny::Submitted(e) => format!("Submitted|{}", e.client_order_id),
            OrderEventAny::Accepted(e) => format!("Accepted|{}", e.client_order_id),
            OrderEventAny::Rejected(e) => format!("Rejected|{}", e.client_order_id),
            OrderEventAny::Canceled(e) => format!("Canceled|{}", e.client_order_id),
            OrderEventAny::Filled(e) => format!(
                "Filled|{}|{:?}|{}|{}|{:?}",
                e.client_order_id, e.order_side, e.last_px, e.last_qty, e.liquidity_side
            ),
            other => format!("Other|{other:?}"),
        })
        .collect()
}

/// UT-BT-02：撮合仿真——市价即成交于对手价，限价到达价位才成交。
#[test]
fn ut_bt_02_matching_fills() {
    case("UT-BT-02", || {
        let instrument = eth_perp();
        let cache = Rc::new(RefCell::new(Cache::default()));
        let (exchange, saved) = build_exchange(cache.clone());
        exchange
            .borrow_mut()
            .add_instrument(InstrumentAny::CryptoPerpetual(instrument.clone()))
            .expect("标的入交易所");

        // 行情：bid 100.00 / ask 101.00
        exchange
            .borrow_mut()
            .process_quote_tick(&quote_for(&instrument, "100.00", "101.00", 1))
            .expect("行情处理");

        // 市价买 1.000 → 立即成交于 ask 101.00（Taker）
        let market = market_order(&instrument, "O-BT-02-MKT", OrderSide::Buy);
        submit(&exchange, &cache, &market, 2);

        let fills: Vec<_> = saved
            .get_messages()
            .iter()
            .filter_map(|e| match e {
                OrderEventAny::Filled(f) => Some(f.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 1, "市价单必须立即成交");
        assert_eq!(fills[0].client_order_id, market.client_order_id());
        assert_eq!(fills[0].last_px, Price::from("101.00"), "市价成交于对手价 ask");
        assert_eq!(fills[0].last_qty, Quantity::from("1.000"));
        assert_eq!(fills[0].liquidity_side, LiquiditySide::Taker);

        // 限价买 99.00：低于市价 → 挂单不成交
        let limit = limit_order(&instrument, "O-BT-02-LMT", OrderSide::Buy, "99.00");
        submit(&exchange, &cache, &limit, 3);
        assert_eq!(
            exchange.borrow().get_open_orders(Some(instrument.id)).len(),
            1,
            "限价单未到价必须挂在簿上"
        );

        // 行情回落 ask 99.00 → 限价成交于 99.00
        exchange
            .borrow_mut()
            .process_quote_tick(&quote_for(&instrument, "99.00", "99.00", 4))
            .expect("行情处理");

        let fills: Vec<_> = saved
            .get_messages()
            .iter()
            .filter_map(|e| match e {
                OrderEventAny::Filled(f) => Some(f.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 2, "限价单到价后成交");
        assert_eq!(fills[1].client_order_id, limit.client_order_id());
        assert_eq!(fills[1].last_px, Price::from("99.00"), "限价成交价不劣于限定价");
        assert_eq!(fills[1].last_qty, Quantity::from("1.000"));
        assert_eq!(fills[1].liquidity_side, LiquiditySide::Maker);
    });
}

/// UT-BT-01：同一输入两次运行事件流逐一致（确定性）。
#[test]
fn ut_bt_01_backtest_determinism() {
    case("UT-BT-01", || {
        let run = || -> Vec<String> {
            let instrument = eth_perp();
            let cache = Rc::new(RefCell::new(Cache::default()));
            let (exchange, saved) = build_exchange(cache.clone());
            exchange
                .borrow_mut()
                .add_instrument(InstrumentAny::CryptoPerpetual(instrument.clone()))
                .expect("标的入交易所");

            // 固定输入序列：行情 → 市价 → 行情 → 限价 → 行情（穿越）
            exchange
                .borrow_mut()
                .process_quote_tick(&quote_for(&instrument, "100.00", "101.00", 1))
                .expect("行情处理");
            let market = market_order(&instrument, "O-BT-01-MKT", OrderSide::Buy);
            submit(&exchange, &cache, &market, 2);
            exchange
                .borrow_mut()
                .process_quote_tick(&quote_for(&instrument, "100.50", "100.75", 3))
                .expect("行情处理");
            let limit = limit_order(&instrument, "O-BT-01-LMT", OrderSide::Sell, "101.50");
            submit(&exchange, &cache, &limit, 4);
            exchange
                .borrow_mut()
                .process_quote_tick(&quote_for(&instrument, "101.50", "101.75", 5))
                .expect("行情处理");

            event_digest(&saved.get_messages())
        };

        let first = run();
        let second = run();

        assert!(!first.is_empty(), "运行必须产生事件流");
        assert!(
            first.iter().any(|line| line.starts_with("Filled|")),
            "事件流必须含成交"
        );
        assert_eq!(first, second, "同一输入两次运行事件流必须逐一致");
    });
}
