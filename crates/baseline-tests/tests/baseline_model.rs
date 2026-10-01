//! UT-MODEL-01/02：订单状态机迁移（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! UT-MODEL-01：合法迁移链 Initialized→Submitted→Accepted→Filled 逐步可达且
//! 状态/数量正确（事件溯源）；
//! UT-MODEL-02：状态机外迁移（未提交即成交、已成交再成交）被拒绝且不产生状态变更。

use nautilus_model::{
    enums::OrderStatus,
    events::OrderEventAny,
    events::order::spec::{
        OrderAcceptedSpec, OrderFilledSpec, OrderInitializedSpec, OrderSubmittedSpec,
    },
    orders::{MarketOrder, Order},
};
use nautilus_research_testkit::case;

/// UT-MODEL-01：合法状态迁移全覆盖（市价单主链）。
#[test]
fn ut_model_01_valid_transitions() {
    case("UT-MODEL-01", || {
        let mut order: MarketOrder = OrderInitializedSpec::builder()
            .quantity(Quantity::from(100_000))
            .build()
            .try_into()
            .expect("初始化市价单");
        assert_eq!(order.status(), OrderStatus::Initialized);
        assert_eq!(order.filled_qty(), Quantity::from(0));
        assert_eq!(order.leaves_qty(), Quantity::from(100_000));

        order
            .apply(OrderEventAny::Submitted(
                OrderSubmittedSpec::builder().build(),
            ))
            .expect("Initialized→Submitted 合法");
        assert_eq!(order.status(), OrderStatus::Submitted);

        order
            .apply(OrderEventAny::Accepted(OrderAcceptedSpec::builder().build()))
            .expect("Submitted→Accepted 合法");
        assert_eq!(order.status(), OrderStatus::Accepted);

        order
            .apply(OrderEventAny::Filled(
                OrderFilledSpec::builder()
                    .last_qty(Quantity::from(100_000))
                    .last_px(Price::from("10.50"))
                    .build(),
            ))
            .expect("Accepted→Filled 合法");
        assert_eq!(order.status(), OrderStatus::Filled);
        assert_eq!(order.filled_qty(), Quantity::from(100_000));
        assert_eq!(order.leaves_qty(), Quantity::from(0));
        assert!(order.is_closed());
    });
}

use nautilus_model::types::{Price, Quantity};

/// UT-MODEL-02：状态机外迁移拒绝且不改状态。
#[test]
fn ut_model_02_invalid_transitions_rejected() {
    case("UT-MODEL-02", || {
        // 未提交（Initialized）直接成交：非法
        let mut order: MarketOrder = OrderInitializedSpec::builder()
            .quantity(Quantity::from(100_000))
            .build()
            .try_into()
            .expect("初始化市价单");
        let result = order.apply(OrderEventAny::Filled(
            OrderFilledSpec::builder()
                .last_qty(Quantity::from(100_000))
                .last_px(Price::from("10.50"))
                .build(),
        ));
        assert!(
            matches!(result, Err(nautilus_model::orders::OrderError::InvalidStateTransition)),
            "Initialized 直接 Filled 必须拒绝：{result:?}"
        );
        assert_eq!(order.status(), OrderStatus::Initialized, "拒绝后状态不变");
        assert_eq!(order.filled_qty(), Quantity::from(0), "拒绝后不得产生成交");

        // 已成交再成交：非法
        order
            .apply(OrderEventAny::Submitted(
                OrderSubmittedSpec::builder().build(),
            ))
            .expect("提交");
        order
            .apply(OrderEventAny::Accepted(OrderAcceptedSpec::builder().build()))
            .expect("接受");
        order
            .apply(OrderEventAny::Filled(
                OrderFilledSpec::builder()
                    .last_qty(Quantity::from(100_000))
                    .last_px(Price::from("10.50"))
                    .build(),
            ))
            .expect("首次成交合法");
        let result = order.apply(OrderEventAny::Filled(
            OrderFilledSpec::builder()
                .last_qty(Quantity::from(50_000))
                .last_px(Price::from("10.60"))
                .build(),
        ));
        assert!(
            result.is_err(),
            "Filled 后重复成交必须拒绝（同 trade_id 报 DuplicateFill）：{result:?}"
        );
        assert_eq!(order.status(), OrderStatus::Filled, "拒绝后状态不变");
        assert_eq!(order.filled_qty(), Quantity::from(100_000), "成交量不被篡改");
    });
}
