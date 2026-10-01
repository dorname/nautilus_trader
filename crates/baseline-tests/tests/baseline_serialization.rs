//! UT-SER-01：事件序列化往返（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! OrderFilled 经 serde_json 序列化后反序列化，内容无损（字段级相等）。

use nautilus_core::UUID4;
use nautilus_model::{
    enums::{LiquiditySide, OrderSide, OrderType},
    events::OrderFilled,
    identifiers::{
        AccountId, ClientOrderId, InstrumentId, StrategyId, TradeId, TraderId, VenueOrderId,
    },
    types::{Currency, Money, Price, Quantity},
};
use nautilus_research_testkit::case;

#[test]
fn ut_ser_01_event_serialization_round_trip() {
    case("UT-SER-01", || {
        let original = OrderFilled::new(
            TraderId::from("TRADER-001"),
            StrategyId::from("EMA-CROSS"),
            InstrumentId::from("600000.SH"),
            ClientOrderId::from("O-19700101-000000-001-001-1"),
            VenueOrderId::from("V-001"),
            AccountId::from("SIM-001"),
            TradeId::from("T-001"),
            OrderSide::Buy,
            OrderType::Market,
            Quantity::from("100"),
            Price::from("10.50"),
            Currency::CNY(),
            LiquiditySide::Taker,
            UUID4::default(),
            nautilus_core::UnixNanos::from(1_000_000_000),
            nautilus_core::UnixNanos::from(2_000_000_000),
            false,
            None,
            Some(Money::new(5.25, Currency::CNY())),
            None,
        );

        let json = serde_json::to_string(&original).expect("序列化成功");
        let deserialized: OrderFilled = serde_json::from_str(&json).expect("反序列化成功");

        assert_eq!(deserialized, original, "往返后事件必须字段级相等");
        assert_eq!(deserialized.trade_id, original.trade_id);
        assert_eq!(deserialized.last_px, original.last_px, "成交价无损");
        assert_eq!(deserialized.last_qty, original.last_qty, "成交数量无损");
        assert_eq!(deserialized.commission, original.commission, "佣金无损");
    });
}
