//! UT-DATA-01/02：数据引擎分发（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-DATA-01：多订阅者多场所订阅——每条行情只派发给订阅了对应标的的订阅者；
//! UT-DATA-02：自定义数据注入——注册自定义类型（CustomDataTrait）并发布，
//! 数据经数据引擎派发且可还原消费。

use std::{any::Any, cell::RefCell, rc::Rc, sync::Arc};

use nautilus_common::{
    cache::Cache,
    clock::VirtualClock,
    msgbus::{
        self,
        stubs::{get_any_saving_handler, get_typed_message_saving_handler},
        switchboard::{get_custom_topic, get_quotes_topic},
    },
};
use nautilus_data::engine::DataEngine;
use nautilus_model::{
    data::{CustomData, CustomDataTrait, Data, HasTsInit, QuoteTick},
    identifiers::InstrumentId,
    types::{Price, Quantity},
};
use nautilus_research_testkit::case;
use serde::Serialize;

fn quote(id: &str, px: &str, ts: u64) -> QuoteTick {
    QuoteTick::new(
        InstrumentId::from(id),
        Price::from(px),
        Price::from(px),
        Quantity::from(1),
        Quantity::from(1),
        nautilus_core::UnixNanos::from(ts),
        nautilus_core::UnixNanos::from(ts),
    )
}

/// UT-DATA-01：订阅分发——订阅者只收到所订阅标的的数据。
#[test]
fn ut_data_01_subscription_dispatch_isolation() {
    case("UT-DATA-01", || {
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let cache = Rc::new(RefCell::new(Cache::new(None, None)));
        let mut engine = DataEngine::new(clock, cache.clone(), None);

        let (handler_aapl, aapl_saved) = get_typed_message_saving_handler::<QuoteTick>(None);
        msgbus::subscribe_quotes(
            get_quotes_topic(InstrumentId::from("AAPL.XNAS")).into(),
            handler_aapl,
            None,
        );
        let (handler_msft, msft_saved) = get_typed_message_saving_handler::<QuoteTick>(None);
        msgbus::subscribe_quotes(
            get_quotes_topic(InstrumentId::from("MSFT.XNAS")).into(),
            handler_msft,
            None,
        );
        // 第二个订阅者订阅同一标的（AAPL）：广播语义
        let (handler_aapl_2, aapl_2_saved) = get_typed_message_saving_handler::<QuoteTick>(None);
        msgbus::subscribe_quotes(
            get_quotes_topic(InstrumentId::from("AAPL.XNAS")).into(),
            handler_aapl_2,
            None,
        );

        // 派发两条不同标的行情
        engine.process_data(Data::Quote(quote("AAPL.XNAS", "190.00", 1)));
        engine.process_data(Data::Quote(quote("MSFT.XNAS", "420.00", 2)));

        let aapl = aapl_saved.get_messages();
        assert_eq!(aapl.len(), 1, "AAPL 订阅者只收到 AAPL 行情");
        assert_eq!(aapl[0].instrument_id, InstrumentId::from("AAPL.XNAS"));
        assert_eq!(aapl[0].bid_price, Price::from("190.00"));

        let msft = msft_saved.get_messages();
        assert_eq!(msft.len(), 1, "MSFT 订阅者只收到 MSFT 行情");
        assert_eq!(msft[0].instrument_id, InstrumentId::from("MSFT.XNAS"));
        assert_eq!(msft[0].bid_price, Price::from("420.00"));

        // 同标的广播：两个 AAPL 订阅者都收到，MSFT 订阅者不受影响
        engine.process_data(Data::Quote(quote("AAPL.XNAS", "191.00", 3)));
        assert_eq!(aapl_saved.get_messages().len(), 2, "AAPL 订阅者收到第二条");
        assert_eq!(
            aapl_2_saved.get_messages().len(),
            2,
            "同标的第二订阅者同步收到"
        );
        assert_eq!(msft_saved.get_messages().len(), 1, "MSFT 订阅者不串扰");

        // 行情同时进入 cache（引擎分发副作用）
        assert_eq!(
            cache.borrow().quote(&InstrumentId::from("AAPL.XNAS")).map(|q| q.bid_price),
            Some(Price::from("191.00")),
            "最新行情已入 cache"
        );
    });
}

/// 自定义数据类型：信号快照（研究侧自定义注入数据）。
#[derive(Debug, Clone, Serialize)]
#[allow(missing_partial_eq)]
struct SignalSnapshot {
    ts_init: nautilus_core::UnixNanos,
    symbol: String,
    score: f64,
}

impl HasTsInit for SignalSnapshot {
    fn ts_init(&self) -> nautilus_core::UnixNanos {
        self.ts_init
    }
}

impl CustomDataTrait for SignalSnapshot {
    fn type_name(&self) -> &'static str {
        "SignalSnapshot"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn ts_event(&self) -> nautilus_core::UnixNanos {
        self.ts_init
    }
    fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(self)?)
    }
    fn clone_arc(&self) -> Arc<dyn CustomDataTrait> {
        Arc::new(self.clone())
    }
    fn eq_arc(&self, other: &dyn CustomDataTrait) -> bool {
        other
            .as_any()
            .downcast_ref::<Self>()
            .is_some_and(|other| self.symbol == other.symbol && self.score == other.score)
    }
}

/// UT-DATA-02：自定义数据注入——注册类型并发布，数据被引擎派发与消费。
#[test]
fn ut_data_02_custom_data_injection() {
    case("UT-DATA-02", || {
        let clock = Rc::new(RefCell::new(VirtualClock::new()));
        let cache = Rc::new(RefCell::new(Cache::new(None, None)));
        let mut engine = DataEngine::new(clock, cache, None);

        // 按 DataType 注册自定义数据订阅
        let data_type = nautilus_model::data::DataType::new("SignalSnapshot", None, None);
        let topic = get_custom_topic(&data_type);
        let (handler, saved) = get_any_saving_handler::<CustomData>(None);
        msgbus::subscribe_any(topic.into(), handler, None);

        // 发布自定义数据
        let payload = SignalSnapshot {
            ts_init: nautilus_core::UnixNanos::from(42),
            symbol: "600000.SH".to_string(),
            score: 0.87,
        };
        engine.process_data(Data::Custom(CustomData::from_arc(Arc::new(payload))));

        let received = saved.get_messages();
        assert_eq!(received.len(), 1, "订阅者收到一条自定义数据");
        let custom = &received[0];
        assert_eq!(
            custom.data_type.type_name(),
            "SignalSnapshot",
            "数据类型标识随数据流转"
        );
        // 还原为具体类型消费
        let restored = custom
            .data
            .as_any()
            .downcast_ref::<SignalSnapshot>()
            .expect("自定义数据可 downcast 还原");
        assert_eq!(restored.symbol, "600000.SH");
        assert!((restored.score - 0.87).abs() < 1e-9);
        // JSON 序列化往返（CustomDataTrait 契约）
        let json = restored.to_json().expect("自定义数据可序列化");
        assert!(json.contains("\"score\":0.87"), "JSON 含字段值：{json}");
        assert_eq!(
            engine.subscribed_custom_data().len(),
            0,
            "本用例直接经 msgbus 订阅，引擎订阅表为空"
        );
    });
}

