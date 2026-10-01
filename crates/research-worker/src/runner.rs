//! 引擎运行编排：日线 EMA 研究的 Nautilus 回测装配（S13）。
//!
//! 隔日执行适配：引擎的市价单在提交瞬间按当前市价成交（撮合引擎同步撮合），
//! 因此策略在 on_bar(T 收盘) 只记录信号；每个交易日额外注入一笔开盘价集合竞价
//! trade tick（06:00 UTC，先于 07:00 的日线 bar），策略在 on_trade_tick 提交
//! 市价单，即以 T+1 开盘价成交——收盘回调的委托不进入同 bar 撮合，
//! 由 ST-S13-01 数值断言证明（信号日收 11 不成交，次日开盘 12 成交）。
//!
//! 引擎含 Rc/RefCell，本模块全部对象在同一调用线程创建、使用、销毁；
//! run_id、墙钟时间不进入运行产物（规范化结果可跨次比对，UT-S13-05）。

use std::{cell::RefCell, fmt::Debug, rc::Rc};

use anyhow::Context as _;
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::actor::DataActor;
use nautilus_execution::models::fee::FeeModelHandle;
use nautilus_model::{
    data::{Bar, BarSpecification, BarType, Data, TradeTick},
    enums::{AccountType, AggressorSide, AggregationSource, BarAggregation, BookType, OmsType, OrderSide, PriceType, TimeInForce},
    events::OrderFilled,
    identifiers::{InstrumentId, Symbol, TradeId, Venue},
    instruments::{Equity, InstrumentAny},
    types::{Currency, Money, Price, Quantity},
};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use ustr::Ustr;

use nautilus_research_domain::{indicators::ema_last, time::date32_to_trade_date};

use crate::{
    fees::{astock_fee, AStockFeeModel},
    gate::{size_buy, BoardRules},
};

/// 一日行情（来自研究快照分区）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayBar {
    pub date: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: u64,
}

/// EMA 运行配置（F-RUN 覆盖：快1慢2、预热20日、槽位1、日调仓）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmaRunConfig {
    pub fast: u32,
    pub slow: u32,
    /// 预热 bar 数：预热期内不交易、指标不参与收益。
    pub warmup_bars: usize,
    pub capital: Decimal,
    /// 费用参数（费率/最低/卖出税/其他），十进制字符串的解析结果。
    pub commission_rate: Decimal,
    pub min_commission: Decimal,
    pub sell_tax_rate: Decimal,
    pub other_fee_rate: Decimal,
    pub rules: BoardRules,
}

/// 一笔成交记录（规范化内容：无 UUID/墙钟）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillRec {
    pub date: String,
    pub side: String,
    pub qty: u64,
    pub price: String,
    pub fee_cny: String,
}

/// 每日收盘估值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquityPoint {
    pub date: String,
    pub cash_cny: String,
    pub position_qty: u64,
    pub close: String,
    pub equity_cny: String,
}

/// 运行产物（规范化、确定性）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutcome {
    pub fills: Vec<FillRec>,
    pub equity_curve: Vec<EquityPoint>,
    pub final_cash_cny: String,
    pub final_equity_cny: String,
    pub total_return: String,
}

/// 日内时间戳：交易日 15:00（Asia/Shanghai）= 07:00 UTC（与快照 available_at 一致）。
fn day_close_ns(date: &str) -> u64 {
    let days = nautilus_research_domain::time::trade_date_to_date32(date).unwrap_or(0);
    (days as i64 * 86_400 + 7 * 3600) as u64 * 1_000_000_000
}

/// 开盘集合竞价时间戳：收盘前 1 小时（先于日线 bar 到达引擎）。
fn day_open_ns(date: &str) -> u64 {
    day_close_ns(date) - 3_600_000_000_000
}

fn date_of_ns(ns: u64) -> String {
    let days = (ns / 1_000_000_000 / 86_400) as i32;
    date32_to_trade_date(days)
}

fn dec(s: impl ToString) -> Decimal {
    Decimal::from_str_exact(&s.to_string()).unwrap_or(Decimal::ZERO)
}

struct Sink {
    fills: Vec<FillRec>,
    cash: Decimal,
    held: u64,
}

/// 日线 EMA 策略：收盘 bar 只形成信号，次日开盘竞价 tick 提交委托。
struct ResearchEmaStrategy {
    core: StrategyCore,
    bar_type: BarType,
    instrument_id: InstrumentId,
    config: EmaRunConfig,
    closes: Vec<Decimal>,
    bar_index: usize,
    /// 收盘形成的待执行信号（方向, 数量），次日开盘 tick 提交。
    pending_signal: Option<(OrderSide, u64)>,
    sink: Rc<RefCell<Sink>>,
}

nautilus_strategy!(ResearchEmaStrategy, {
    fn on_order_filled(&mut self, event: &OrderFilled) {
        let mut sink = self.sink.borrow_mut();
        let qty = dec(event.last_qty.to_string());
        let px = dec(event.last_px.to_string());
        let fee = event.commission.map(|c| c.as_decimal()).unwrap_or(Decimal::ZERO);
        sink.fills.push(FillRec {
            date: date_of_ns(event.ts_event.as_u64()),
            side: match event.order_side {
                OrderSide::Buy => "buy",
                _ => "sell",
            }
            .to_string(),
            qty: qty.normalize().to_string().parse().unwrap_or(0),
            price: px.normalize().to_string(),
            fee_cny: fee.normalize().to_string(),
        });
        match event.order_side {
            OrderSide::Buy => {
                sink.cash -= qty * px + fee;
                sink.held += qty.to_string().parse::<u64>().unwrap_or(0);
            }
            _ => {
                sink.cash += qty * px - fee;
                sink.held = sink.held.saturating_sub(qty.to_string().parse::<u64>().unwrap_or(0));
            }
        }
    }

    fn on_order_rejected(&mut self, _event: nautilus_model::events::OrderRejected) {}
});

impl Debug for ResearchEmaStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResearchEmaStrategy")
            .field("instrument_id", &self.instrument_id)
            .finish()
    }
}

impl DataActor for ResearchEmaStrategy {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_bars(self.bar_type, None, None);
        self.subscribe_trades(self.instrument_id, None, None);
        Ok(())
    }

    /// 开盘集合竞价：执行昨日收盘形成的信号（隔日执行的唯一成交入口）。
    fn on_trade(&mut self, tick: &TradeTick) -> anyhow::Result<()> {
        let Some((side, qty)) = self.pending_signal.take() else {
            return Ok(());
        };
        // T+1：当日买入的库存当日不可卖
        let qty = if side == OrderSide::Sell {
            let today = date_of_ns(tick.ts_event.as_u64());
            let sink = self.sink.borrow();
            let bought_today: u64 = sink
                .fills
                .iter()
                .filter(|f| f.date == today && f.side == "buy")
                .map(|f| f.qty)
                .sum();
            qty.min(sink.held.saturating_sub(bought_today))
        } else {
            qty
        };
        if qty == 0 {
            return Ok(());
        }
        let order = self.order().market(
            self.instrument_id,
            side,
            Quantity::from(qty),
            Some(TimeInForce::Gtc),
            None,
            None,
            None,
            None,
            None,
            None,
        );
        self.submit_order(order, None, None, None)?;
        Ok(())
    }

    /// 收盘 bar：只形成信号，不直接下单。
    fn on_bar(&mut self, bar: &Bar) -> anyhow::Result<()> {
        self.bar_index += 1;
        self.closes.push(dec(bar.close.to_string()));
        // 预热期不交易（指标以初始资金为起点，不含未来数据）
        if self.bar_index <= self.config.warmup_bars {
            return Ok(());
        }
        let (Some(fast), Some(slow)) = (
            ema_last(&self.closes, self.config.fast),
            ema_last(&self.closes, self.config.slow),
        ) else {
            return Ok(());
        };
        if self.pending_signal.is_some() {
            return Ok(());
        }
        let (held, cash) = {
            let sink = self.sink.borrow();
            (sink.held, sink.cash)
        };
        let close = dec(bar.close.to_string());
        if fast > slow && held == 0 {
            // 收盘信号按当日收盘估算数量，次日开盘成交（隔日执行）
            let fee_of = |q: u64| {
                let turnover = Decimal::from(q) * close;
                turnover
                    + astock_fee(
                        false,
                        turnover,
                        self.config.commission_rate,
                        self.config.min_commission,
                        self.config.sell_tax_rate,
                        self.config.other_fee_rate,
                    )
            };
            let qty = size_buy(cash, close, u64::MAX / 2, &self.config.rules, fee_of);
            if qty > 0 {
                self.pending_signal = Some((OrderSide::Buy, qty));
            }
        } else if fast < slow && held > 0 {
            self.pending_signal = Some((OrderSide::Sell, held));
        }
        Ok(())
    }
}

/// 执行一次日线 EMA 研究运行（同线程创建/使用/销毁引擎）。
pub fn run_ema_daily(
    instrument_code: &str,
    venue_name: &str,
    bars: &[DayBar],
    config: &EmaRunConfig,
) -> anyhow::Result<RunOutcome> {
    anyhow::ensure!(!bars.is_empty(), "运行区间无行情");
    let mut engine = BacktestEngine::new(BacktestEngineConfig::default())?;

    let currency = Currency::from("CNY");
    let mut venue_config = SimulatedVenueConfig::builder()
        .venue(Venue::from(venue_name))
        .oms_type(OmsType::Netting)
        .account_type(AccountType::Cash)
        .book_type(BookType::L1_MBP)
        .starting_balances(vec![Money::new(
            config.capital.to_string().parse::<f64>().context("资本金额")?,
            currency,
        )])
        .fee_model(
            nautilus_execution::models::fee::FeeModelAny::MakerTaker(
                nautilus_execution::models::fee::MakerTakerFeeModel::zero(),
            )
            .into(),
        )
        .build()?;
    // 注入 A 股费用模型（builder 仅接受内置枚举，构造后替换句柄）
    venue_config.fee_model = FeeModelHandle::from_rc(Rc::new(AStockFeeModel {
        commission_rate: config.commission_rate,
        min_commission: config.min_commission,
        sell_tax_rate: config.sell_tax_rate,
        other_fee_rate: config.other_fee_rate,
        currency,
    }));
    engine.add_venue(venue_config)?;

    // 精度取自最小变动价位（tick 的小数位）：bar 价格按此精度格式化，
    // 否则撮合引擎以「精度不符」跳过全部 bar（No market → 拒单）
    let precision = config.rules.tick.scale() as usize;
    let px_of = |d: &Decimal| Price::from(format!("{d:.precision$}").as_str());

    let instrument_id = InstrumentId::from(format!("{instrument_code}.{venue_name}").as_str());
    let equity = Equity::builder()
        .instrument_id(instrument_id)
        .raw_symbol(Symbol::from(instrument_code))
        .isin(Ustr::from("SYNTHETIC000"))
        .currency(currency)
        .price_precision(precision as u8)
        .price_increment(px_of(&config.rules.tick))
        .lot_size(Quantity::from(config.rules.min_qty.max(1)))
        .ts_event(Default::default())
        .ts_init(Default::default())
        .build()
        .map_err(|e| anyhow::anyhow!("合成标的构建失败：{e}"))?;
    engine.add_instrument(&InstrumentAny::Equity(equity))?;

    let bar_type = BarType::new(
        instrument_id,
        BarSpecification::new(1, BarAggregation::Day, PriceType::Last),
        AggregationSource::External,
    );
    // 每个交易日两条数据：开盘集合竞价 trade tick（06:00 UTC）+ 日线 bar（07:00 UTC），
    // 策略在 tick 上执行昨日信号 → 成交价为当日开盘价
    let mut data: Vec<Data> = Vec::with_capacity(bars.len() * 2);
    for b in bars {
        let open_ts = day_open_ns(&b.date);
        data.push(Data::Trade(TradeTick::new(
            instrument_id,
            px_of(&b.open),
            Quantity::from(b.volume.max(1)),
            AggressorSide::NoAggressor,
            TradeId::from(format!("OPEN{}", b.date.replace('-', "")).as_str()),
            open_ts.into(),
            open_ts.into(),
        )));
        let ts = day_close_ns(&b.date);
        data.push(Data::Bar(Bar::new(
            bar_type,
            px_of(&b.open),
            px_of(&b.high),
            px_of(&b.low),
            px_of(&b.close),
            Quantity::from(b.volume),
            ts.into(),
            ts.into(),
        )));
    }

    let sink = Rc::new(RefCell::new(Sink {
        fills: Vec::new(),
        cash: config.capital,
        held: 0,
    }));
    let strategy = ResearchEmaStrategy {
        core: StrategyCore::new(StrategyConfig {
            strategy_id: Some(nautilus_model::identifiers::StrategyId::from("RESEARCH-EMA-001")),
            order_id_tag: Some("001".to_string()),
            ..Default::default()
        }),
        bar_type,
        instrument_id,
        config: config.clone(),
        closes: Vec::new(),
        bar_index: 0,
        pending_signal: None,
        sink: Rc::clone(&sink),
    };
    engine.add_strategy(strategy)?;
    engine.add_data(data, None, true, true)?;
    engine.run(None, None, None, false)?;

    // 收盘估值序列：现金 + 持仓 × 当日收盘（填充重放，确定性）
    let sink = sink.borrow();
    let mut cash = config.capital;
    let mut held = 0u64;
    let mut equity_curve = Vec::with_capacity(bars.len());
    for b in bars {
        for f in sink.fills.iter().filter(|f| f.date == b.date) {
            let qty: u64 = f.qty;
            let px = dec(&f.price);
            let fee = dec(&f.fee_cny);
            if f.side == "buy" {
                cash -= Decimal::from(qty) * px + fee;
                held += qty;
            } else {
                cash += Decimal::from(qty) * px - fee;
                held = held.saturating_sub(qty);
            }
        }
        let equity = cash + Decimal::from(held) * b.close;
        equity_curve.push(EquityPoint {
            date: b.date.clone(),
            cash_cny: cash.normalize().to_string(),
            position_qty: held,
            close: b.close.normalize().to_string(),
            equity_cny: equity.normalize().to_string(),
        });
    }
    let final_equity = equity_curve.last().map(|e| dec(&e.equity_cny)).unwrap_or(config.capital);
    let total_return = (final_equity / config.capital) - Decimal::ONE;
    Ok(RunOutcome {
        fills: sink.fills.clone(),
        equity_curve,
        final_cash_cny: cash.normalize().to_string(),
        final_equity_cny: final_equity.normalize().to_string(),
        total_return: total_return.normalize().to_string(),
    })
}
