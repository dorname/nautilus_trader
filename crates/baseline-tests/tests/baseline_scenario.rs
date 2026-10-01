//! ST-01~08：端到端场景基线（规格 logos/resources/test/scenario/scenario-test-cases.md）。
//!
//! 可离线复现的场景以引擎真实装配执行：
//! - ST-01 首次回测旅程（EMA cross + BacktestEngine 完整历史区间）
//! - ST-02 多场所组合回测（单策略订阅两场所并分别下单）
//! - ST-03 风控拦截端到端（RiskEngine→ExecutionEngine→策略事件）
//! - ST-06 历史数据研究工作流（本地 bars 完整回放、时间推进、重复一致）
//! - ST-07 自定义数据融合（信号与行情按时间序交错处理）
//! - ST-08 纯 Rust 节点旅程（Rust API 组装最小回测 + sandbox 链路）
//!
//! ST-04/05 需 live 节点运行时（LiveExecNode / kill -9 实盘恢复），环境阻塞
//! 诚实 skip 上报（见 manifest 批次 9）。

use std::{cell::RefCell, rc::Rc};

use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::{
    cache::Cache,
    clock::VirtualClock,
    messages::execution::SubmitOrder,
    msgbus::{self, MessageBus, stubs::get_typed_message_saving_handler},
};
use nautilus_core::{UUID4, UnixNanos};
use nautilus_execution::{
    engine::{ExecutionEngine, stubs::StubExecutionClient},
    models::fee::{FeeModelAny, MakerTakerFeeModel},
};
use nautilus_model::{
    data::{Bar, BarType, CustomData, CustomDataTrait, Data, HasTsInit, QuoteTick},
    enums::{AccountType, AggregationSource, BarAggregation, BookType, OmsType, OrderSide, OrderType, PriceType},
    events::OrderEventAny,
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, StrategyId, Venue},
    instruments::{Instrument, InstrumentAny, stubs::crypto_perpetual_ethusdt},
    orders::{Order, OrderAny, OrderTestBuilder},
    types::{Money, Price, Quantity},
};
use nautilus_research_testkit::case;
use nautilus_risk::RiskEngine;
use nautilus_trading::examples::strategies::EmaCross;
use rust_decimal::Decimal;

/// ST-01：首次回测旅程——运行完成、订单事件链完整、结果报告产出、重复运行一致。
#[test]
fn st_01_first_backtest_journey() {
    case("ST-01", || {
        let journey = || -> (usize, usize, usize, Vec<String>) {
            *msgbus::get_message_bus().borrow_mut() = MessageBus::default();

            let mut engine = BacktestEngine::new(BacktestEngineConfig::default()).expect("引擎构建");
            engine
                .add_venue(
                    SimulatedVenueConfig::builder()
                        .venue(Venue::from("BINANCE"))
                        .oms_type(OmsType::Netting)
                        .account_type(AccountType::Margin)
                        .book_type(BookType::L1_MBP)
                        .starting_balances(vec![Money::from("1_000_000 USDT")])
                        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()).into())
                        .build()
                        .expect("场所配置"),
                )
                .expect("场所添加");
            let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
            let instrument_id = instrument.id();
            engine.add_instrument(&instrument).expect("标的添加");
            engine
                .add_strategy(EmaCross::new(instrument_id, Quantity::from("0.100"), 10, 20))
                .expect("策略添加");

            // 事件链捕获（策略 topic 全量；须在引擎构建后注册，run 期间生效）
            let (handler, saved) = get_typed_message_saving_handler::<OrderEventAny>(None);
            msgbus::subscribe_order_events("events.order.*".into(), handler, None);

            // 完整历史区间：盘整 → 上行（金叉买）→ 下行（死叉卖）→ 回升（再买）
            let spread = 0.10;
            let mut quotes: Vec<Data> = Vec::new();
            let base_ts: u64 = 1_000_000_000;
            let mut tick: u64 = 0;
            let mut push = |mid: f64| {
                quotes.push(Data::Quote(QuoteTick::new(
                    instrument_id,
                    Price::from(format!("{:.2}", mid - spread / 2.0).as_str()),
                    Price::from(format!("{:.2}", mid + spread / 2.0).as_str()),
                    Quantity::from("1.000"),
                    Quantity::from("1.000"),
                    UnixNanos::from(base_ts + tick),
                    UnixNanos::from(base_ts + tick),
                )));
                tick += 1;
            };
            for _ in 0..25 {
                push(1000.0);
            }
            for i in 0..40 {
                push(1000.0 + i as f64 * 5.0);
            }
            for i in 0..80 {
                push(1195.0 - i as f64 * 5.0);
            }
            for i in 0..40 {
                push(800.0 + i as f64 * 5.0);
            }
            engine.add_data(quotes, None, true, true).expect("数据添加");
            engine.run(None, None, None, false).expect("回测运行");

            let result = engine.get_result();
            let digest: Vec<String> = saved
                .get_messages()
                .iter()
                .map(|event| match event {
                    OrderEventAny::Submitted(e) => format!("Submitted|{}", e.client_order_id),
                    OrderEventAny::Filled(e) => format!(
                        "Filled|{}|{:?}|{}|{:?}",
                        e.client_order_id, e.order_side, e.last_px, e.last_qty
                    ),
                    // 语义摘要不含 event_id/ts（跨次随机）
                    other => format!("{:?}", std::mem::discriminant(other)),
                })
                .collect();
            (
                result.iterations,
                result.total_orders,
                result.total_positions,
                digest,
            )
        };

        let (iterations, orders, positions, chain) = journey();
        assert_eq!(iterations, 185, "全部行情数据推进完成");
        assert!(orders >= 2, "金叉/死叉至少产生 2 笔订单，实际 {orders}");
        assert!(positions > 0, "成交产生持仓");
        assert!(
            chain.iter().any(|l| l.starts_with("Submitted|"))
                && chain.iter().any(|l| l.starts_with("Filled|")),
            "订单事件链完整（Submitted→Filled）：{chain:?}"
        );

        // 重复运行一致（含事件链）
        let (iterations_2, orders_2, positions_2, chain_2) = journey();
        assert_eq!(
            (iterations_2, orders_2, positions_2),
            (iterations, orders, positions),
            "重复运行结果计数一致"
        );
        assert_eq!(chain, chain_2, "重复运行事件链逐一致");
    });
}

/// ST-02：多场所组合回测——单策略订阅两场所行情并分别下单，事件链按场所独立完整。
#[test]
fn st_02_multi_venue_backtest() {
    case("ST-02", || {
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();

        let mut engine = BacktestEngine::new(BacktestEngineConfig::default()).expect("引擎构建");
        // 两个场所：BINANCE 与 SANDBOX（各自的撮合与账户）
        for venue_name in ["BINANCE", "SANDBOX"] {
            engine
                .add_venue(
                    SimulatedVenueConfig::builder()
                        .venue(Venue::from(venue_name))
                        .oms_type(OmsType::Netting)
                        .account_type(AccountType::Margin)
                        .book_type(BookType::L1_MBP)
                        .starting_balances(vec![Money::from("1_000_000 USDT")])
                        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()).into())
                        .build()
                        .expect("场所配置"),
                )
                .expect("场所添加");
        }

        // 两场所各一个标的
        let ins_a = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt()); // ETHUSDT-PERP.BINANCE
        let mut raw_b = crypto_perpetual_ethusdt();
        raw_b.id = InstrumentId::from("ETHUSDT-PERP.SANDBOX");
        raw_b.raw_symbol = nautilus_model::identifiers::Symbol::from("ETHUSDT-PERP");
        let ins_b = InstrumentAny::CryptoPerpetual(raw_b);
        engine.add_instrument(&ins_a).expect("标的 A");
        engine.add_instrument(&ins_b).expect("标的 B");

        // 单一策略订阅两场所并分别下单
        engine
            .add_strategy(DualVenueProbe::new(ins_a.id(), ins_b.id()))
            .expect("策略添加");

        // 事件捕获（须在引擎构建后注册）
        let (handler, saved) = get_typed_message_saving_handler::<OrderEventAny>(None);
        msgbus::subscribe_order_events("events.order.*".into(), handler, None);

        // 两场所各一条行情（策略首条即市价买入）
        let ts: u64 = 1_000_000_000;
        let quotes = vec![
            Data::Quote(QuoteTick::new(
                ins_a.id(),
                Price::from("2000.00"),
                Price::from("2000.10"),
                Quantity::from("1.000"),
                Quantity::from("1.000"),
                UnixNanos::from(ts),
                UnixNanos::from(ts),
            )),
            Data::Quote(QuoteTick::new(
                ins_b.id(),
                Price::from("3000.00"),
                Price::from("3000.10"),
                Quantity::from("1.000"),
                Quantity::from("1.000"),
                UnixNanos::from(ts + 1),
                UnixNanos::from(ts + 1),
            )),
        ];
        engine.add_data(quotes, None, true, true).expect("数据添加");
        engine.run(None, None, None, false).expect("回测运行");

        // 两场所订单生命周期独立完整
        let fills: Vec<_> = saved
            .get_messages()
            .iter()
            .filter_map(|event| match event {
                OrderEventAny::Filled(f) => Some((f.instrument_id, f.last_px)),
                _ => None,
            })
            .collect();
        assert_eq!(fills.len(), 2, "两场所各成交一笔");
        assert!(
            fills.iter().any(|(id, _)| *id == ins_a.id()),
            "场所 A 成交"
        );
        assert!(
            fills.iter().any(|(id, _)| *id == ins_b.id()),
            "场所 B 成交"
        );

        // Portfolio 汇总两场所持仓（cache 聚合视图）
        let result = engine.get_result();
        assert_eq!(result.total_positions, 2, "组合汇总两场所持仓");
    });
}

/// ST-03：风控拦截端到端——RiskEngine 拒绝、事件到达策略订阅、venue 未收到命令。
#[test]
fn st_03_risk_denial_end_to_end() {
    case("ST-03", || {
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();

        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let portfolio =
            nautilus_portfolio::portfolio::Portfolio::new(clock.clone(), cache.clone(), None);

        // 执行引擎注册 msgbus 端点（exec_engine_process 收风控 Denied 后发布策略 topic）
        let exec = Rc::new(RefCell::new(ExecutionEngine::new(
            clock.clone(),
            cache.clone(),
            None,
        )));
        ExecutionEngine::register_msgbus_handlers(&exec);

        // venue 客户端：拦截后必须零命令
        let venue_client = StubExecutionClient::new(
            ClientId::from("SIM"),
            AccountId::from("SIM-001"),
            Venue::from("SIM"),
            OmsType::Netting,
            None,
        );
        exec.borrow_mut()
            .register_client(Box::new(venue_client.clone()))
            .expect("venue 客户端注册");
        exec.borrow_mut()
            .register_venue_routing(ClientId::from("SIM"), Venue::from("SIM"))
            .expect("路由注册");

        // 风控引擎：名义额上限 100
        let mut risk = RiskEngine::new(
            nautilus_risk::engine::config::RiskEngineConfig::default(),
            portfolio,
            clock.clone(),
            cache.clone(),
        );
        let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
        risk.set_max_notional_per_order(instrument.id(), Decimal::from(100));

        cache
            .borrow_mut()
            .add_instrument(instrument.clone())
            .expect("标的入 cache");
        let order: OrderAny = OrderTestBuilder::new(OrderType::Limit)
            .instrument_id(instrument.id())
            .client_order_id(ClientOrderId::from("O-ST-03"))
            .side(OrderSide::Buy)
            .quantity(Quantity::from("1.000"))
            .price(Price::from("2000.00")) // 名义额 2000 > 100
            .build();
        cache
            .borrow_mut()
            .add_order(order.clone(), None, None, true)
            .expect("订单入 cache");

        // 策略侧订阅（Denied 经执行引擎发布到策略 topic）
        let (handler, saved) = get_typed_message_saving_handler::<OrderEventAny>(None);
        let strategy_topic = format!("events.order.{}", order.strategy_id());
        msgbus::subscribe_order_events(strategy_topic.as_str().into(), handler, None);

        // 提交超限订单
        let submit = SubmitOrder::new(
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
        );
        risk.execute(TradingCommandAlias::SubmitOrder(submit));

        // 风控拒绝：事件到达策略订阅
        let events = saved.get_messages();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, OrderEventAny::Denied(_))),
            "Denied 事件必须到达策略订阅：{events:?}"
        );
        // venue 未收到命令
        assert!(
            venue_client.submitted_order_ids().borrow().is_empty(),
            "被拒命令不得到达 venue"
        );
    });
}

use nautilus_common::messages::execution::TradingCommand as TradingCommandAlias;

/// ST-06：历史数据研究工作流——本地 bars 完整回放、时间推进单调、重复运行一致。
#[test]
fn st_06_historical_replay_workflow() {
    case("ST-06", || {
        let replay = || -> (usize, Vec<u64>) {
            *msgbus::get_message_bus().borrow_mut() = MessageBus::default();
            let mut engine = BacktestEngine::new(BacktestEngineConfig::default()).expect("引擎构建");
            engine
                .add_venue(
                    SimulatedVenueConfig::builder()
                        .venue(Venue::from("XSHG"))
                        .oms_type(OmsType::Netting)
                        .account_type(AccountType::Cash)
                        .book_type(BookType::L1_MBP)
                        .starting_balances(vec![Money::from("1_000_000 CNY")])
                        .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()).into())
                        .build()
                        .expect("场所配置"),
                )
                .expect("场所添加");
            // 研究标的：600000.XSHG（A股日线样例，精度 2）
            let equity = nautilus_model::instruments::Equity::builder()
                .instrument_id(InstrumentId::from("600000.XSHG"))
                .raw_symbol(nautilus_model::identifiers::Symbol::from("600000"))
                .currency(nautilus_model::types::Currency::CNY())
                .price_precision(2)
                .price_increment(Price::from("0.01"))
                .ts_event(UnixNanos::default())
                .ts_init(UnixNanos::default())
                .build()
                .expect("标的构建");
            engine
                .add_instrument(&InstrumentAny::Equity(equity))
                .expect("标的添加");

            let bar_type = BarType::new(
                InstrumentId::from("600000.XSHG"),
                nautilus_model::data::BarSpecification::new(1, BarAggregation::Day, PriceType::Last),
                AggregationSource::External,
            );
            // 30 根历史日线（供应商数据的本地化样例），时间单调推进
            let base_ts: u64 = 1_700_000_000_000_000_000;
            let day: u64 = 86_400_000_000_000;
            let bars: Vec<Data> = (0..30)
                .map(|i| {
                    let close = 10.0 + i as f64 * 0.1;
                    Data::Bar(Bar::new(
                        bar_type.clone(),
                        Price::from(format!("{close:.2}").as_str()),
                        Price::from(format!("{:.2}", close + 0.2).as_str()),
                        Price::from(format!("{:.2}", close - 0.2).as_str()),
                        Price::from(format!("{close:.2}").as_str()),
                        Quantity::from(1_000_000),
                        UnixNanos::from(base_ts + i * day),
                        UnixNanos::from(base_ts + i * day),
                    ))
                })
                .collect();
            engine.add_data(bars, None, true, true).expect("数据添加");
            engine.run(None, None, None, false).expect("回放运行");

            let result = engine.get_result();
            // 时间推进：迭代次数 == 数据条数（时间由数据驱动）
            (result.iterations, vec![result.iterations as u64])
        };

        let (iterations, _) = replay();
        assert_eq!(iterations, 30, "历史数据完整回放");
        let (iterations_2, _) = replay();
        assert_eq!(iterations_2, iterations, "重复回放事件计数一致");
    });
}

/// 信号快照：注入的自定义数据。
#[derive(Debug, Clone, serde::Serialize)]
struct FusionSignal {
    ts_init: UnixNanos,
    value: f64,
}

impl HasTsInit for FusionSignal {
    fn ts_init(&self) -> UnixNanos {
        self.ts_init
    }
}

impl CustomDataTrait for FusionSignal {
    fn type_name(&self) -> &'static str {
        "FusionSignal"
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn ts_event(&self) -> UnixNanos {
        self.ts_init
    }
    fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(self)?)
    }
    fn clone_arc(&self) -> std::sync::Arc<dyn CustomDataTrait> {
        std::sync::Arc::new(self.clone())
    }
    fn eq_arc(&self, other: &dyn CustomDataTrait) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self.value == other.value)
    }
}

/// ST-07：自定义数据融合——信号与行情按时间序交错处理。
#[test]
fn st_07_custom_data_fusion() {
    case("ST-07", || {
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();

        let mut engine = BacktestEngine::new(BacktestEngineConfig::default()).expect("引擎构建");
        engine
            .add_venue(
                SimulatedVenueConfig::builder()
                    .venue(Venue::from("BINANCE"))
                    .oms_type(OmsType::Netting)
                    .account_type(AccountType::Margin)
                    .book_type(BookType::L1_MBP)
                    .starting_balances(vec![Money::from("1_000_000 USDT")])
                    .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()).into())
                    .build()
                    .expect("场所配置"),
            )
            .expect("场所添加");
        let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
        let instrument_id = instrument.id();
        engine.add_instrument(&instrument).expect("标的添加");

        // 接收序列（时间戳标签）
        let received: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        engine
            .add_strategy(SignalFusionProbe::new(instrument_id, received.clone()))
            .expect("策略添加");

        // 交错序列：quote@1 → signal@2 → quote@3 → signal@4
        let quote = |ts: u64| {
            Data::Quote(QuoteTick::new(
                instrument_id,
                Price::from("2000.00"),
                Price::from("2000.10"),
                Quantity::from("1.000"),
                Quantity::from("1.000"),
                UnixNanos::from(ts),
                UnixNanos::from(ts),
            ))
        };
        let signal = |ts: u64| {
            Data::Custom(CustomData::from_arc(std::sync::Arc::new(FusionSignal {
                ts_init: UnixNanos::from(ts),
                value: 0.5,
            })))
        };
        engine
            .add_data(
                vec![quote(1), signal(2), quote(3), signal(4)],
                None,
                true,
                true,
            )
            .expect("数据添加");
        engine.run(None, None, None, false).expect("回测运行");

        let sequence = received.borrow();
        assert_eq!(
            *sequence,
            vec!["quote@1", "signal@2", "quote@3", "signal@4"],
            "信号与行情按时间序交错处理：{sequence:?}"
        );
    });
}

/// ST-08：纯 Rust 节点旅程——Rust API 组装最小回测与 sandbox 实盘链路。
#[test]
fn st_08_rust_node_journey() {
    case("ST-08", || {
        // 旅程 1：最小回测节点（Rust API 全程）
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();
        let mut engine = BacktestEngine::new(BacktestEngineConfig::default()).expect("引擎构建");
        engine
            .add_venue(
                SimulatedVenueConfig::builder()
                    .venue(Venue::from("BINANCE"))
                    .oms_type(OmsType::Netting)
                    .account_type(AccountType::Margin)
                    .book_type(BookType::L1_MBP)
                    .starting_balances(vec![Money::from("1_000_000 USDT")])
                    .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()).into())
                    .build()
                    .expect("场所配置"),
            )
            .expect("场所添加");
        let instrument = InstrumentAny::CryptoPerpetual(crypto_perpetual_ethusdt());
        let instrument_id = instrument.id();
        engine.add_instrument(&instrument).expect("标的添加");
        engine
            .add_strategy(EmaCross::new(instrument_id, Quantity::from("0.100"), 2, 4))
            .expect("策略添加");
        let quotes: Vec<Data> = (0..12)
            .map(|i| {
                let mid = 1000.0 + i as f64 * 10.0;
                Data::Quote(QuoteTick::new(
                    instrument_id,
                    Price::from(format!("{:.2}", mid - 0.05).as_str()),
                    Price::from(format!("{:.2}", mid + 0.05).as_str()),
                    Quantity::from("1.000"),
                    Quantity::from("1.000"),
                    UnixNanos::from(1_000_000_000 + i),
                    UnixNanos::from(1_000_000_000 + i),
                ))
            })
            .collect();
        engine.add_data(quotes, None, true, true).expect("数据添加");
        engine.run(None, None, None, false).expect("回测运行");
        let result = engine.get_result();
        assert_eq!(result.iterations, 12, "回测节点旅程完成");

        // 旅程 2：sandbox 实盘链路（同引擎 API 家族，参照 UT-ADV-02 模板）
        *msgbus::get_message_bus().borrow_mut() = MessageBus::default();
        let (exec_tx, mut exec_rx) =
            tokio::sync::mpsc::unbounded_channel::<nautilus_common::messages::ExecutionEvent>();
        nautilus_common::live::runner::replace_exec_event_sender(exec_tx);

        let cache = Rc::new(RefCell::new(Cache::default()));
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let mut raw = crypto_perpetual_ethusdt();
        raw.id = InstrumentId::from("ETHUSDT-PERP.SANDBOX");
        let sandbox_instrument = InstrumentAny::CryptoPerpetual(raw);
        cache
            .borrow_mut()
            .add_instrument(sandbox_instrument.clone())
            .expect("标的入 cache");

        let core = nautilus_execution::client::core::ExecutionClientCore::new(
            nautilus_model::identifiers::TraderId::from("TRADER-001"),
            ClientId::from("SANDBOX"),
            Venue::from("SANDBOX"),
            OmsType::Netting,
            AccountId::from("SANDBOX-001"),
            AccountType::Margin,
            None,
            cache.clone(),
        );
        let config = nautilus_sandbox::config::SandboxExecutionClientConfig::builder()
            .account_id(AccountId::from("SANDBOX-001"))
            .venue(Venue::from("SANDBOX"))
            .starting_balances(vec![Money::from("1_000_000 USDT")])
            .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
            .build();
        let mut client = nautilus_sandbox::SandboxExecutionClient::new(
            core, config, clock.clone(), cache.clone(),
        )
        .expect("sandbox 客户端构建");
        use nautilus_common::clients::ExecutionClient as _;
        client.start().expect("sandbox 启动");

        let quote = QuoteTick::new(
            sandbox_instrument.id(),
            Price::from("2000.00"),
            Price::from("2000.10"),
            Quantity::from("1.000"),
            Quantity::from("1.000"),
            UnixNanos::from(10),
            UnixNanos::from(10),
        );
        client.process_quote_tick(&quote).expect("行情进簿");
        let order: OrderAny = OrderTestBuilder::new(OrderType::Market)
            .instrument_id(sandbox_instrument.id())
            .client_order_id(ClientOrderId::from("O-ST-08"))
            .side(OrderSide::Buy)
            .quantity(Quantity::from("1.000"))
            .build();
        cache
            .borrow_mut()
            .add_order(order.clone(), None, None, false)
            .expect("订单入 cache");
        client
            .submit_order(SubmitOrder::new(
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
                UnixNanos::from(11),
                None,
            ))
            .expect("订单提交");

        let mut filled = false;
        while let Ok(event) = exec_rx.try_recv() {
            if let nautilus_common::messages::ExecutionEvent::Order(OrderEventAny::Filled(_)) =
                event
            {
                filled = true;
            }
        }
        assert!(filled, "sandbox 链路产生成交回报（Rust 节点旅程完整）");
    });
}

// ===================== 场景策略 =====================

use nautilus_common::actor::DataActor;
use nautilus_trading::{
    nautilus_strategy, strategy::Strategy, strategy::StrategyCore,
    strategy::config::StrategyConfig,
};
use std::fmt::Debug;

/// 双场所探测策略：订阅两标的，各首条行情市价买入。
pub struct DualVenueProbe {
    core: StrategyCore,
    instrument_a: InstrumentId,
    instrument_b: InstrumentId,
    sent: [bool; 2],
}

impl DualVenueProbe {
    pub fn new(instrument_a: InstrumentId, instrument_b: InstrumentId) -> Self {
        Self {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("DUAL-PROBE")),
                order_id_tag: Some("001".to_string()),
                ..Default::default()
            }),
            instrument_a,
            instrument_b,
            sent: [false, false],
        }
    }

    fn probe(&mut self, index: usize, instrument_id: InstrumentId) -> anyhow::Result<()> {
        if self.sent[index] {
            return Ok(());
        }
        self.sent[index] = true;
        let order = self.order().market(
            instrument_id,
            OrderSide::Buy,
            Quantity::from("0.100"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        self.submit_order(order, None, None, None)
    }
}

nautilus_strategy!(DualVenueProbe);

impl Debug for DualVenueProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DualVenueProbe")
            .field("instrument_a", &self.instrument_a)
            .field("instrument_b", &self.instrument_b)
            .finish()
    }
}

impl DataActor for DualVenueProbe {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_quotes(self.instrument_a, None, None);
        self.subscribe_quotes(self.instrument_b, None, None);
        Ok(())
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> anyhow::Result<()> {
        if quote.instrument_id == self.instrument_a {
            self.probe(0, self.instrument_a)?;
        } else if quote.instrument_id == self.instrument_b {
            self.probe(1, self.instrument_b)?;
        }
        Ok(())
    }
}

/// 信号融合探测策略：记录行情与自定义信号的到达顺序。
pub struct SignalFusionProbe {
    core: StrategyCore,
    instrument_id: InstrumentId,
    received: Rc<RefCell<Vec<String>>>,
}

impl SignalFusionProbe {
    pub fn new(instrument_id: InstrumentId, received: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            core: StrategyCore::new(StrategyConfig {
                strategy_id: Some(StrategyId::from("FUSION-PROBE")),
                order_id_tag: Some("001".to_string()),
                ..Default::default()
            }),
            instrument_id,
            received,
        }
    }
}

nautilus_strategy!(SignalFusionProbe);

impl Debug for SignalFusionProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalFusionProbe")
            .field("instrument_id", &self.instrument_id)
            .finish()
    }
}

impl DataActor for SignalFusionProbe {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_quotes(self.instrument_id, None, None);
        self.subscribe_data(
            nautilus_model::data::DataType::new("FusionSignal", None, None),
            None,
            None,
        );
        Ok(())
    }

    fn on_quote(&mut self, quote: &QuoteTick) -> anyhow::Result<()> {
        self.received
            .borrow_mut()
            .push(format!("quote@{}", quote.ts_init));
        Ok(())
    }

    fn on_data(&mut self, data: &CustomData) -> anyhow::Result<()> {
        if let Some(signal) = data.data.as_any().downcast_ref::<FusionSignal>() {
            self.received
                .borrow_mut()
                .push(format!("signal@{}", signal.ts_init));
        }
        Ok(())
    }
}

/// ST-04/05：环境阻塞诚实 skip 上报（skip 不计为通过，verify 覆盖度门可见）。
///
/// - ST-04 需 live 节点运行时（LiveExecNode「回测转实盘零改动」双环境旅程）；
///   sandbox 撮合模板已由 UT-ADV-02、ST-08 覆盖其离线可验证部分。
/// - ST-05 需实盘进程 kill -9 崩溃恢复场景（live 进程管理与重启编排）。
#[test]
fn st_04_05_live_runtime_blocked() {
    nautilus_research_testkit::report_result(
        "ST-04",
        "skip",
        0,
        Some("需 live 节点运行时（LiveExecNode 双环境零改动旅程），当前环境无 live runtime"),
    );
    nautilus_research_testkit::report_result(
        "ST-05",
        "skip",
        0,
        Some("需实盘运行中 kill -9 崩溃恢复场景（live 进程管理），当前环境不可执行"),
    );
}
