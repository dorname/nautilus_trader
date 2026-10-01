//! A股费用模型：佣金（费率+最低）、卖出印花税、其他费用（S13-AC-02）。
//!
//! 纯函数 `astock_fee` 是数值真相源（UT-S13-03）；`AStockFeeModel` 把它接入
//! Nautilus 引擎（FeeModel 的 commission 即该笔成交的全部费用）。
//! 价格/数量经定点字符串转换，不走 f64，保证十进制精确。

use nautilus_model::orders::Order as _;
use rust_decimal::Decimal;

/// 单笔成交费用（十进制精确）。
/// - 佣金 = max(成交额 × 佣金率, 最低佣金)，买卖双向；
/// - 印花税 = 成交额 × 卖出税率，仅卖出；
/// - 其他费用 = 成交额 × 其他费率，买卖双向。
pub fn astock_fee(
    is_sell: bool,
    turnover: Decimal,
    commission_rate: Decimal,
    min_commission: Decimal,
    sell_tax_rate: Decimal,
    other_fee_rate: Decimal,
) -> Decimal {
    let commission = (turnover * commission_rate).max(min_commission);
    let tax = if is_sell { turnover * sell_tax_rate } else { Decimal::ZERO };
    let other = turnover * other_fee_rate;
    commission + tax + other
}

/// 引擎侧费用模型（RC 注入 SimulatedVenueConfig.fee_model）。
#[derive(Debug)]
pub struct AStockFeeModel {
    pub commission_rate: Decimal,
    pub min_commission: Decimal,
    pub sell_tax_rate: Decimal,
    pub other_fee_rate: Decimal,
    pub currency: nautilus_model::types::Currency,
}

impl nautilus_execution::models::fee::FeeModel for AStockFeeModel {
    fn get_commission(
        &self,
        order: &nautilus_model::orders::OrderAny,
        fill_quantity: nautilus_model::types::Quantity,
        fill_px: nautilus_model::types::Price,
        _instrument: &nautilus_model::instruments::InstrumentAny,
    ) -> anyhow::Result<nautilus_model::types::Money> {
        let turnover = decimal_of(&fill_px.to_string())? * decimal_of(&fill_quantity.to_string())?;
        let fee = astock_fee(
            order.order_side() == nautilus_model::enums::OrderSide::Sell,
            turnover,
            self.commission_rate,
            self.min_commission,
            self.sell_tax_rate,
            self.other_fee_rate,
        );
        // 费用保留两位（CNY 精度）：超出精度四舍五入
        let fee = fee.round_dp(2);
        Ok(nautilus_model::types::Money::new(
            fee.to_string().parse::<f64>().unwrap_or(0.0),
            self.currency,
        ))
    }
}

fn decimal_of(s: &str) -> anyhow::Result<Decimal> {
    // nautilus 定点 Display 形如 "12.00" 或 "100"；去掉千分位等修饰
    let cleaned: String = s.chars().filter(|c| *c == '-' || *c == '.' || c.is_ascii_digit()).collect();
    Decimal::from_str_exact(&cleaned).map_err(|e| anyhow::anyhow!("定点数转换失败 {s}：{e}"))
}
