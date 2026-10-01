//! UT-EXEC-01/02：执行引擎（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-EXEC-01：命令分发路由——多场所订单命令按 instrument venue 路由到
//! 对应 ExecutionClient，互不串扰；
//! UT-EXEC-02：执行事件回写——venue 回报经引擎处理后 Cache 更新且事件
//! 发布到消息总线。

use std::{cell::RefCell, rc::Rc};

use nautilus_common::{
    cache::Cache,
    clock::VirtualClock,
    messages::execution::{SubmitOrder, TradingCommand},
    msgbus::{
        self, MessageBus,
        stubs::get_typed_message_saving_handler,
    },
};
use nautilus_core::{UUID4, UnixNanos};
use nautilus_execution::engine::{ExecutionEngine, stubs::StubExecutionClient};
use nautilus_model::{
    enums::{OmsType, OrderSide, OrderStatus, OrderType},
    events::OrderEventAny,
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, Venue},
    instruments::{
        InstrumentAny,
        stubs::{audusd_sim, equity_aapl, gbpusd_sim},
    },
    orders::{Order, OrderAny, OrderTestBuilder, stubs::TestOrderEventStubs},
    types::Quantity,
};
use nautilus_research_testkit::case;

fn market_order(instrument_id: InstrumentId, client_order_id: &str, qty: u64) -> OrderAny {
    OrderTestBuilder::new(OrderType::Market)
        .instrument_id(instrument_id)
        .client_order_id(ClientOrderId::from(client_order_id))
        .side(OrderSide::Buy)
        .quantity(Quantity::from(qty))
        .build()
}

fn submit_command(order: &OrderAny) -> TradingCommand {
    TradingCommand::SubmitOrder(SubmitOrder::new(
        order.trader_id(),
        None,
        order.strategy_id(),
        order.instrument_id(),
        order.client_order_id(),
        order.init_event().clone(),
        None,
        None,
        None,
        UUID4::new(),
        UnixNanos::default(),
        None,
    ))
}

/// UT-EXEC-01：多场所命令按 venue 路由到目标 ExecutionClient。
#[test]
fn ut_exec_01_command_routing_by_venue() {
    case("UT-EXEC-01", || {
        // 消息总线单例隔离
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();

        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let mut engine = ExecutionEngine::new(clock, cache.clone(), None);

        // 两个场所各注册一个执行客户端
        let client_xnas = StubExecutionClient::new(
            ClientId::from("XNAS"),
            AccountId::from("XNAS-001"),
            Venue::from("XNAS"),
            OmsType::Netting,
            None,
        );
        let client_shex = StubExecutionClient::new(
            ClientId::from("SHEX"),
            AccountId::from("SHEX-001"),
            Venue::from("SHEX"),
            OmsType::Netting,
            None,
        );
        engine
            .register_client(Box::new(client_xnas.clone()))
            .expect("XNAS 客户端注册");
        engine
            .register_client(Box::new(client_shex.clone()))
            .expect("SHEX 客户端注册");
        engine
            .register_venue_routing(ClientId::from("XNAS"), Venue::from("XNAS"))
            .expect("XNAS 路由注册");
        engine
            .register_venue_routing(ClientId::from("SHEX"), Venue::from("SHEX"))
            .expect("SHEX 路由注册");

        // 两个场所各一个标的
        let ins_a = InstrumentAny::Equity(equity_aapl()); // AAPL.XNAS
        let mut raw_b = gbpusd_sim();
        raw_b.id = InstrumentId::from("GBP/USD.SHEX");
        let ins_b = InstrumentAny::CurrencyPair(raw_b); // GBP/USD.SHEX
        cache
            .borrow_mut()
            .add_instrument(ins_a)
            .expect("标的 A 入 cache");
        cache
            .borrow_mut()
            .add_instrument(ins_b)
            .expect("标的 B 入 cache");

        // 两笔订单分别属于两个场所
        let order_a = market_order(InstrumentId::from("AAPL.XNAS"), "O-EXEC-01-A", 100);
        let order_b = market_order(InstrumentId::from("GBP/USD.SHEX"), "O-EXEC-01-B", 10_000);
        cache
            .borrow_mut()
            .add_order(order_a.clone(), None, None, true)
            .expect("订单 A 入 cache");
        cache
            .borrow_mut()
            .add_order(order_b.clone(), None, None, true)
            .expect("订单 B 入 cache");

        engine.execute(submit_command(&order_a));
        engine.execute(submit_command(&order_b));

        // 命令按 venue 精确路由，互不串扰
        let routed_xnas = client_xnas.submitted_order_ids();
        let routed_shex = client_shex.submitted_order_ids();
        assert_eq!(
            routed_xnas.borrow().as_slice(),
            &[order_a.client_order_id()],
            "SIM 场所订单必须路由到 SIM 客户端"
        );
        assert_eq!(
            routed_shex.borrow().as_slice(),
            &[order_b.client_order_id()],
            "SHEX 场所订单必须路由到 SHEX 客户端"
        );
    });
}

/// UT-EXEC-02：venue 回报经引擎回写 Cache 并发布总线。
#[test]
fn ut_exec_02_event_writeback_and_publish() {
    case("UT-EXEC-02", || {
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();

        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let mut engine = ExecutionEngine::new(clock, cache.clone(), None);
        let client = StubExecutionClient::new(
            ClientId::from("SIM"),
            AccountId::from("SIM-001"),
            Venue::from("SIM"),
            OmsType::Netting,
            None,
        );
        engine
            .register_client(Box::new(client.clone()))
            .expect("客户端注册");
        engine
            .register_venue_routing(ClientId::from("SIM"), Venue::from("SIM"))
            .expect("SIM 路由注册");

        let ins = InstrumentAny::CurrencyPair(audusd_sim());
        cache.borrow_mut().add_instrument(ins).expect("标的入 cache");
        let order = market_order(InstrumentId::from("AUD/USD.SIM"), "O-EXEC-02", 100_000);
        cache
            .borrow_mut()
            .add_order(order.clone(), None, None, true)
            .expect("订单入 cache");

        // 事件总线捕获（策略 topic 与场所 topic 两个 pattern）
        let (handler, saved) = get_typed_message_saving_handler::<OrderEventAny>(None);
        msgbus::subscribe_order_events("events.order.*".into(), handler.clone(), None);
        msgbus::subscribe_order_events("events.order_submitted.*".into(), handler, None);

        // 命令下发 → venue 回报 Submitted 到达引擎
        engine.execute(submit_command(&order));
        engine.process(&TestOrderEventStubs::submitted(
            &order,
            AccountId::from("SIM-001"),
        ));

        // 回写：Cache 中订单状态已更新
        let cached_status = {
            let cache_ref = cache.borrow();
            cache_ref
                .order(&order.client_order_id())
                .expect("订单在 cache")
                .status()
        };
        assert_eq!(
            cached_status,
            OrderStatus::Submitted,
            "venue 回报必须回写 Cache 订单状态"
        );
        // 命令确实到达了目标客户端
        assert_eq!(
            client.submitted_order_ids().borrow().as_slice(),
            &[order.client_order_id()],
            "命令下发到目标客户端"
        );

        // 发布：总线上捕获到 Submitted 事件（策略 topic + 场所 topic）
        let events = saved.get_messages();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, OrderEventAny::Submitted(_))),
            "事件必须发布到总线：{events:?}"
        );
        assert!(
            events.len() >= 2,
            "Submitted 事件经策略与场所双 topic 发布，实际 {} 条",
            events.len()
        );
    });
}
