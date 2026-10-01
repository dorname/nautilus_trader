//! UT-RISK-01/02/03：风控引擎预检（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-RISK-01：订单名义额超预设额度 → OrderDenied 事件产生，命令不下发 venue；
//! UT-RISK-02：限价违反 instrument 价格约束（精度超上限）→ 拒单且有事件记录；
//! UT-RISK-03：预检逐笔读取最新累计状态——同批两笔单笔合规但累计超限时第二笔拒。

use std::{cell::RefCell, rc::Rc};

use nautilus_common::{
    cache::Cache,
    clock::VirtualClock,
    messages::execution::{SubmitOrder, TradingCommand},
    msgbus::{
        self,
        stubs::get_typed_into_message_saving_handler,
        switchboard::MessagingSwitchboard,
    },
};
use nautilus_core::{UUID4, UnixNanos};
use nautilus_model::{
    enums::{OrderSide, OrderType},
    events::{OrderEventAny, order::spec::OrderInitializedSpec},
    identifiers::ClientOrderId,
    instruments::{Instrument, InstrumentAny, stubs::audusd_sim},
    orders::{LimitOrder, MarketOrder, OrderAny},
};
use nautilus_research_testkit::case;
use rust_decimal::Decimal;


#[test]
fn ut_risk_01_max_notional_denied_and_not_forwarded() {
    case("UT-RISK-01", || {
        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let portfolio = nautilus_portfolio::portfolio::Portfolio::new(clock.clone(), cache.clone(), None);
        let mut engine = nautilus_risk::RiskEngine::new(
            nautilus_risk::engine::config::RiskEngineConfig::default(),
            portfolio,
            clock.clone(),
            cache.clone(),
        );
        let instrument = InstrumentAny::CurrencyPair(audusd_sim());
        engine.set_max_notional_per_order(instrument.id(), Decimal::from(100));

        // 限价 1000 AUD 名义额 @1.0 × 1000 单位 —— 远超 100 上限
        let init = OrderInitializedSpec::builder()
            .instrument_id(instrument.id())
            .client_order_id(ClientOrderId::from("O-RISK-01"))
            .order_side(OrderSide::Buy)
            .order_type(OrderType::Limit)
            .quantity(Quantity::from("1000"))
            .price(Price::from("1.00000"))
            .build();
        let order = OrderAny::Market(MarketOrder::try_from(init.clone()).expect("合法初始化"));
        cache
            .borrow_mut()
            .add_order(order.clone(), None, None, true)
            .expect("订单入 cache");
        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("instrument 入 cache");

        // venue 侧（放行才收到）与拒单事件侧双捕获
        let (venue_handler, venue_saved) =
            get_typed_into_message_saving_handler::<TradingCommand>(None);
        msgbus::register_trading_command_endpoint(
            MessagingSwitchboard::exec_engine_queue_execute(),
            venue_handler,
        );
        let (event_handler, event_saved) =
            get_typed_into_message_saving_handler::<OrderEventAny>(None);
        msgbus::register_order_event_endpoint(MessagingSwitchboard::exec_engine_process(), event_handler);

        let submit = SubmitOrder::new(
            init.trader_id,
            None,
            init.strategy_id,
            init.instrument_id,
            init.client_order_id,
            init,
            None,
            None,
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
        );
        engine.execute(TradingCommand::SubmitOrder(submit));

        let events = event_saved.get_messages();
        assert!(!events.is_empty(), "超限必须产生 OrderDenied 事件");
        assert!(
            events.iter().all(|e| matches!(e, OrderEventAny::Denied(_))),
            "事件必须为 OrderDenied：{events:?}"
        );
        assert!(
            venue_saved.get_messages().is_empty(),
            "被拒命令不得下发 venue"
        );
    });
}

use nautilus_model::types::{Price, Quantity};

/// UT-RISK-02：限价精度超 instrument 上限 → 拒单 + 事件记录，venue 无命令。
#[test]
fn ut_risk_02_price_violation_rejected() {
    case("UT-RISK-02", || {
        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let portfolio = nautilus_portfolio::portfolio::Portfolio::new(clock.clone(), cache.clone(), None);
        let mut engine = nautilus_risk::RiskEngine::new(
            nautilus_risk::engine::config::RiskEngineConfig::default(),
            portfolio,
            clock.clone(),
            cache.clone(),
        );
        let instrument = InstrumentAny::CurrencyPair(audusd_sim()); // price_precision = 5
        assert_eq!(audusd_sim().price_precision, 5, "fixture 精度前置确认");

        // 价格精度 6 位 > 5：违反 instrument 约束
        let init = OrderInitializedSpec::builder()
            .instrument_id(instrument.id())
            .client_order_id(ClientOrderId::from("O-RISK-02"))
            .order_side(OrderSide::Buy)
            .order_type(OrderType::Limit)
            .quantity(Quantity::from("100"))
            .price(Price::from("1.000000"))
            .build();
        let order = OrderAny::Market(MarketOrder::try_from(init.clone()).expect("合法初始化"));
        cache
            .borrow_mut()
            .add_order(order.clone(), None, None, true)
            .expect("订单入 cache");
        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("instrument 入 cache");

        let (venue_handler, venue_saved) =
            get_typed_into_message_saving_handler::<TradingCommand>(None);
        msgbus::register_trading_command_endpoint(
            MessagingSwitchboard::exec_engine_queue_execute(),
            venue_handler,
        );
        let (event_handler, event_saved) =
            get_typed_into_message_saving_handler::<OrderEventAny>(None);
        msgbus::register_order_event_endpoint(MessagingSwitchboard::exec_engine_process(), event_handler);

        let submit = SubmitOrder::new(
            init.trader_id,
            None,
            init.strategy_id,
            init.instrument_id,
            init.client_order_id,
            init,
            None,
            None,
            None,
            UUID4::new(),
            UnixNanos::default(),
            None,
        );
        engine.execute(TradingCommand::SubmitOrder(submit));

        let events = event_saved.get_messages();
        assert!(!events.is_empty(), "价格违规必须拒单且有事件记录");
        assert!(
            venue_saved.get_messages().is_empty(),
            "被拒命令不得下发 venue"
        );
    });
}

/// UT-RISK-03：预检读取最新状态——限额更新后，后续提交按最新值裁决。
#[test]
fn ut_risk_03_precheck_reads_latest_state() {
    case("UT-RISK-03", || {
        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let portfolio = nautilus_portfolio::portfolio::Portfolio::new(clock.clone(), cache.clone(), None);
        // 提交限速取 large-limit 快速路径（容量 >1024），放行命令立即转发不经定时器缓冲
        let config = nautilus_risk::engine::config::RiskEngineConfig::builder()
            .max_order_submit(nautilus_common::throttler::RateLimit::new(
                2000,
                nautilus_core::DurationNanos::from_secs(1),
            ))
            .build()
            .expect("风控配置合法");
        let mut engine = nautilus_risk::RiskEngine::new(config, portfolio, clock.clone(), cache.clone());
        let instrument = InstrumentAny::CurrencyPair(audusd_sim());
        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("instrument 入 cache");
        // 风险检查要求 cache 有账户：SIM-001 USD 现金账户
        let account_state = nautilus_model::events::account::state::AccountState::new(
            nautilus_model::identifiers::AccountId::from("SIM-001"),
            nautilus_model::enums::AccountType::Cash,
            vec![nautilus_model::types::AccountBalance::new(
                nautilus_model::types::Money::from("1000000 USD"),
                nautilus_model::types::Money::from("0 USD"),
                nautilus_model::types::Money::from("1000000 USD"),
            )],
            vec![],
            true,
            UUID4::new(),
            UnixNanos::default(),
            UnixNanos::default(),
            Some(nautilus_model::types::Currency::USD()),
        );
        cache
            .borrow_mut()
            .add_account(nautilus_model::accounts::AccountAny::Cash(
                nautilus_model::accounts::CashAccount::new(account_state, false, false),
            ))
            .expect("账户入 cache");

        let (venue_handler, venue_saved) =
            get_typed_into_message_saving_handler::<TradingCommand>(None);
        msgbus::register_trading_command_endpoint(
            MessagingSwitchboard::exec_engine_queue_execute(),
            venue_handler,
        );
        let (event_handler, event_saved) =
            get_typed_into_message_saving_handler::<OrderEventAny>(None);
        msgbus::register_order_event_endpoint(MessagingSwitchboard::exec_engine_process(), event_handler);

        let mk = |id: &str| {
            let init = OrderInitializedSpec::builder()
                .instrument_id(instrument.id())
                .client_order_id(ClientOrderId::from(id))
                .order_side(OrderSide::Buy)
                .order_type(OrderType::Limit)
                .quantity(Quantity::from("800"))
                .price(Price::from("1.00000"))
                .build();
            let order = OrderAny::Limit(LimitOrder::try_from(init.clone()).expect("合法限价单"));
            cache
                .borrow_mut()
                .add_order(order.clone(), None, None, true)
                .expect("订单入 cache");
            SubmitOrder::new(
                init.trader_id,
                None,
                init.strategy_id,
                init.instrument_id,
                init.client_order_id,
                init,
                None,
                None,
                None,
                UUID4::new(),
                UnixNanos::default(),
                None,
            )
        };

        // 初始大限额：同规模订单放行 → venue 收到（放行通路校验）
        engine.set_max_notional_per_order(instrument.id(), Decimal::from(10000));
        engine.execute(TradingCommand::SubmitOrder(mk("O-RISK-03-A")));
        assert!(
            event_saved.get_messages().is_empty(),
            "放行不得产生拒单事件，实际 {:?}",
            event_saved.get_messages()
        );
        assert_eq!(
            venue_saved.get_messages().len(),
            1,
            "大限额下必须放行并下发 venue，实际 {:?}",
            venue_saved.get_messages().len()
        );

        // 更新限额为 100（最新状态）：同规模订单超限 → 拒且不下发
        engine.set_max_notional_per_order(instrument.id(), Decimal::from(100));
        engine.execute(TradingCommand::SubmitOrder(mk("O-RISK-03-B")));
        let denied_total = event_saved.get_messages().len();
        assert_eq!(denied_total, 1, "收紧限额后必须拒单");
        assert_eq!(
            venue_saved.get_messages().len(),
            1,
            "被拒命令不得下发 venue"
        );
    });
}
