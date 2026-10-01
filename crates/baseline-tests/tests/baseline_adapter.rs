//! UT-ADV-01/02：适配器（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-ADV-01：venue 报文解析——architect_ax 适配器样例报文（instrument 与
//! candle）解析为标准领域对象；
//! UT-ADV-02：sandbox 模板验证——sandbox 执行客户端完整链路（订阅行情 →
//! 提交订单 → 撮合回报），作为适配器测试模板可复用。

use std::{cell::RefCell, rc::Rc};

use nautilus_architect_ax::{
    common::enums::{AxCategory, AxInstrumentState},
    http::{
        models::{AxCandle, AxInstrument},
        parse::{parse_bar, parse_instrument},
    },
};
use nautilus_common::{
    cache::Cache,
    clients::ExecutionClient,
    clock::VirtualClock,
    live::runner::replace_exec_event_sender,
    messages::ExecutionEvent,
    msgbus,
};
use nautilus_core::{UUID4, UnixNanos};
use nautilus_execution::client::core::ExecutionClientCore;
use nautilus_model::{
    enums::{LiquiditySide, OmsType, OrderSide, OrderType},
    events::OrderEventAny,
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, TraderId, Venue},
    instruments::{
        Instrument, InstrumentAny,
        stubs::audusd_sim,
    },
    orders::{Order, OrderAny, OrderTestBuilder},
    types::{Currency, Money, Price, Quantity},
};
use nautilus_research_testkit::case;
use nautilus_sandbox::{SandboxExecutionClient, config::SandboxExecutionClientConfig};
use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
use rust_decimal::Decimal;

/// 样例报文：Ax 1 分钟 candle（JSON，字段名 snake_case）。
const CANDLE_JSON: &str = r#"{
    "symbol": "EURUSD-PERP",
    "ts": 1700000000,
    "open": "1.00000",
    "high": "1.00100",
    "low": "0.99900",
    "close": "1.00050",
    "buy_volume": 120,
    "sell_volume": 80,
    "volume": 200,
    "width": "1m"
}"#;

/// UT-ADV-01：venue 报文解析为标准领域对象。
#[test]
fn ut_adv_01_venue_message_parsing() {
    case("UT-ADV-01", || {
        // 1) candle 报文 → Bar
        let candle: AxCandle =
            serde_json::from_str(CANDLE_JSON).expect("candle 报文可反序列化");
        let instrument = InstrumentAny::CurrencyPair(audusd_sim()); // AUD/USD.SIM，精度 5
        let bar = parse_bar(&candle, &instrument, UnixNanos::from(1)).expect("candle 解析为 Bar");
        assert_eq!(bar.instrument_id(), InstrumentId::from("AUD/USD.SIM"));
        assert_eq!(bar.open, Price::from("1.00000"), "开价无损");
        assert_eq!(bar.high, Price::from("1.00100"), "最高价无损");
        assert_eq!(bar.low, Price::from("0.99900"), "最低价无损");
        assert_eq!(bar.close, Price::from("1.00050"), "收盘价无损");
        assert_eq!(bar.volume, Quantity::from(200), "成交量无损");
        assert_eq!(
            u64::from(bar.ts_event),
            1_700_000_000_u64 * 1_000_000_000,
            "秒级时间戳换算为纳秒"
        );

        // 2) instrument 报文 → 标准领域对象（FX 永续 → CurrencyPair）
        let ax_instrument = sample_ax_instrument();
        let parsed = parse_instrument(&ax_instrument, UnixNanos::from(1), UnixNanos::from(1))
            .expect("instrument 报文解析");
        assert!(
            matches!(parsed, InstrumentAny::PerpetualContract(_)),
            "FX 永续解析为 PerpetualContract：{parsed:?}"
        );
        assert_eq!(parsed.id(), InstrumentId::from("EURUSD-PERP.AX"));
        assert_eq!(
            parsed.price_increment(),
            Price::from("0.0001"),
            "tick size 无损"
        );
    });
}

/// 样例报文：Ax instrument（FX 永续）。
fn sample_ax_instrument() -> AxInstrument {
    AxInstrument {
        symbol: "EURUSD-PERP".into(),
        product: Some("EURUSD".into()),
        state: AxInstrumentState::Open,
        multiplier: Decimal::from(1),
        minimum_order_size: Decimal::from(100),
        tick_size: Decimal::new(1, 4), // 0.0001
        quote_currency: "USD".into(),
        funding_settlement_currency: "USD".into(),
        category: AxCategory::Fx,
        maintenance_margin_pct: Decimal::from(4),
        initial_margin_pct: Decimal::from(8),
        contract_mark_price: None,
        contract_size: None,
        description: Some("Euro / US Dollar FX Perpetual Future".to_string()),
        expiration: None,
        funding_calendar_schedule: None,
        funding_frequency: None,
        funding_rate_cap_lower_pct: Some(Decimal::new(-1, 0)),
        funding_rate_cap_upper_pct: Some(Decimal::from(1)),
        price_band_lower_deviation_pct: Some(Decimal::from(10)),
        price_band_upper_deviation_pct: Some(Decimal::from(10)),
        price_bands: None,
        price_quotation: None,
        underlying_benchmark_price: None,
        price_scale: None,
        additional_product_specs: Default::default(),
        delisted_at: None,
        is_closing: false,
        estimated_funding_supported: false,
        funding_schedule_calendar_description: None,
        funding_schedule_time_description: None,
        funding_schedule: None,
        trading_schedule: None,
    }
}

/// UT-ADV-02：sandbox 执行客户端链路（适配器测试模板）。
#[test]
fn ut_adv_02_sandbox_pipeline() {
    case("UT-ADV-02", || {
        *msgbus::get_message_bus().borrow_mut() = msgbus::MessageBus::default();

        // 事件通道：sandbox 事件经 thread-local sender 送出
        let (exec_tx, mut exec_rx) = tokio::sync::mpsc::unbounded_channel::<ExecutionEvent>();
        replace_exec_event_sender(exec_tx);

        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));

        // 标的入 cache（sandbox 撮合引擎按 cache 中的 instrument 构建）
        let mut raw = {
            use nautilus_model::instruments::stubs::currency_pair_btcusdt;
            let mut pair = currency_pair_btcusdt();
            pair.id = InstrumentId::from("BTCUSDT.SANDBOX");
            pair.raw_symbol = nautilus_model::identifiers::Symbol::from("BTCUSDT");
            pair
        };
        raw.ts_event = UnixNanos::from(1);
        raw.ts_init = UnixNanos::from(1);
        let instrument = InstrumentAny::CurrencyPair(raw);
        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("标的入 cache");

        let core = ExecutionClientCore::new(
            TraderId::from("TRADER-001"),
            ClientId::from("SANDBOX"),
            Venue::from("SANDBOX"),
            OmsType::Netting,
            AccountId::from("SANDBOX-001"),
            nautilus_model::enums::AccountType::Margin,
            None,
            cache.clone(),
        );
        let config = SandboxExecutionClientConfig::builder()
            .account_id(AccountId::from("SANDBOX-001"))
            .venue(Venue::from("SANDBOX"))
            .starting_balances(vec![Money::new(1_000_000.0, Currency::USDT())])
            .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
            .build();
        let mut client =
            SandboxExecutionClient::new(core, config, clock.clone(), cache.clone())
                .expect("sandbox 客户端构建");
        client.start().expect("sandbox 客户端启动");

        // 行情进簿：bid 50000 / ask 50001
        let quote = nautilus_model::data::QuoteTick::new(
            instrument.id(),
            Price::from("50000.00"),
            Price::from("50001.00"),
            Quantity::from("1.000000"),
            Quantity::from("1.000000"),
            UnixNanos::from(2),
            UnixNanos::from(2),
        );
        client
            .process_quote_tick(&quote)
            .expect("行情进 sandbox 撮合");

        // 提交市价买 1 BTC → 立即成交于 ask
        let order: OrderAny = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(instrument.id())
            .client_order_id(ClientOrderId::from("O-ADV-02-MKT"))
            .side(OrderSide::Buy)
            .quantity(Quantity::from("1.000000"))
            .build();
        cache
            .borrow_mut()
            .add_order(order.clone(), None, None, false)
            .expect("订单入 cache");
        let cmd = nautilus_common::messages::execution::SubmitOrder::new(
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
            UnixNanos::from(3),
            None,
        );
        client.submit_order(cmd).expect("订单提交");

        // 收取回报事件
        let mut fill = None;
        while let Ok(event) = exec_rx.try_recv() {
            if let ExecutionEvent::Order(OrderEventAny::Filled(f)) = event {
                fill = Some(f);
            }
        }
        let fill = fill.expect("sandbox 撮合必须产生成交回报");
        assert_eq!(fill.client_order_id, order.client_order_id());
        assert_eq!(fill.last_px, Price::from("50001.00"), "市价成交于对手价 ask");
        assert_eq!(fill.last_qty, Quantity::from("1.000000"));
        assert_eq!(fill.liquidity_side, LiquiditySide::Taker);
    });
}
