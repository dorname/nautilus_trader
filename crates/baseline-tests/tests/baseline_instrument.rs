//! UT-MODEL-03：多资产工具精度换算（规格 logos/resources/test/unit/unit-test-cases.md）。
//!
//! 覆盖现货（FX/加密货币对）、期货、期权、预测市场四类 Instrument：
//! 最小变动价位 = 10^(−价格精度) 的换算不变式逐类成立，乘数/批量等
//! 契约参数与规格一致。

use nautilus_model::instruments::{
    Instrument,
    stubs::{audusd_sim, betting, crypto_option_btc_deribit, futures_contract_es},
};
use nautilus_research_testkit::case;

/// 换算不变式：price_increment == 10^(−price_precision)（按小数位展开比对）。
fn check_increment_matches_precision(name: &str, price_precision: u8, increment: &str) {
    let expected = format!("1e-{}", price_precision);
    let expected_dec: rust_decimal::Decimal = expected
        .parse()
        .unwrap_or_else(|e| panic!("{name}: 换算期望值解析失败 {e}"));
    let actual_dec: rust_decimal::Decimal = increment
        .parse()
        .unwrap_or_else(|e| panic!("{name}: 最小变动价位解析失败 {e}"));
    assert_eq!(
        actual_dec, expected_dec,
        "{name}: 最小变动价位应等于 10^(−{price_precision})"
    );
}

/// UT-MODEL-03：多资产工具精度换算。
#[test]
fn ut_model_03_multi_asset_precision_conversion() {
    case("UT-MODEL-03", || {
        use nautilus_model::instruments::InstrumentAny;

        // 现货（FX）：AUD/USD.SIM，价格精度 5 → 最小变动 0.00001
        let spot = InstrumentAny::CurrencyPair(audusd_sim());
        assert_eq!(spot.price_precision(), 5);
        check_increment_matches_precision("现货 AUD/USD", spot.price_precision(), "0.00001");
        assert_eq!(spot.size_precision(), 0);
        assert_eq!(spot.size_increment(), nautilus_model::types::Quantity::from(1));

        // 期货：ESZ21.GLBX，价格精度 2 → 最小变动 0.01，乘数 1、批量 1
        let future = InstrumentAny::FuturesContract(futures_contract_es(None, None));
        assert_eq!(future.price_precision(), 2);
        check_increment_matches_precision("期货 ES", future.price_precision(), "0.01");
        assert_eq!(future.multiplier(), nautilus_model::types::Quantity::from(1));
        assert_eq!(future.lot_size(), Some(nautilus_model::types::Quantity::from(1)));

        // 期权：BTC-13JAN23-16000-P.DERIBIT，价格精度 3 → 0.001，数量精度 1 → 0.1
        let option = InstrumentAny::CryptoOption(crypto_option_btc_deribit(
            3,
            1,
            nautilus_model::types::Price::from("0.001"),
            nautilus_model::types::Quantity::from("0.1"),
        ));
        assert_eq!(option.price_precision(), 3);
        check_increment_matches_precision("期权 BTC Put", option.price_precision(), "0.001");
        assert_eq!(option.size_precision(), 1);
        assert_eq!(
            option.size_increment(),
            nautilus_model::types::Quantity::from("0.1")
        );

        // 预测市场：BETFAIR，价格精度 = 最小变动价位小数位（动态一致）
        let betting_instrument = InstrumentAny::Betting(betting());
        let betting_increment = betting_instrument.price_increment();
        assert_eq!(
            u8::from(betting_increment.precision),
            betting_instrument.price_precision(),
            "预测市场：价格精度必须等于最小变动价位的小数位"
        );
        check_increment_matches_precision(
            "预测市场 BETFAIR",
            betting_instrument.price_precision(),
            &betting_increment.to_string(),
        );
        // 预测市场价格有界：1.00 ~ 100.00（概率语义）
        assert_eq!(
            betting_instrument.max_price(),
            Some(nautilus_model::types::Price::from("100.00"))
        );
        assert_eq!(
            betting_instrument.min_price(),
            Some(nautilus_model::types::Price::from("1.00"))
        );
    });
}
