"""A股 EMA 双均线策略 — 端到端回测验证（a-stock 分支）。

数据：data/astock_catalog（391 万 bar，通达信 .day → 引擎刻度 Parquet）
标的：600000.SH 浦发银行（2023-01-03 ~ 2026-09-28，906 个交易日）
策略：EMA(5)≥EMA(20) 持多；下穿 平多（纯多头，符合 A股无做空惯例，lot=100）

运行：
  PYTHONPATH=python .tools/pythons/cpython-3.12.14-linux-x86_64-gnu/bin/python3 \
      examples/astock/run_ema_backtest.py
"""

from __future__ import annotations

import sys
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "python"))

from nautilus_trader.backtest import BacktestEngine
from nautilus_trader.backtest import BacktestEngineConfig
from nautilus_trader.config import StrategyConfig
from nautilus_trader.execution import MakerTakerFeeModel
from nautilus_trader.indicators import ExponentialMovingAverage
from nautilus_trader.model import AccountType
from nautilus_trader.model import Bar
from nautilus_trader.model import BarType
from nautilus_trader.model import Currency
from nautilus_trader.model import InstrumentId
from nautilus_trader.model import Money
from nautilus_trader.model import TraderId
from nautilus_trader.model import OmsType
from nautilus_trader.model import OrderSide
from nautilus_trader.trading import Strategy


INSTRUMENT_ID = InstrumentId.from_str("600000.SH")
BAR_TYPE = BarType.from_str("600000.SH-1-DAY-LAST-EXTERNAL")
FAST, SLOW = 5, 20
CASH = 10_000_000  # 1000 万 CNY


class EmaCrossAstkConfig(StrategyConfig):
    def __init__(
        self,
        *,
        instrument_id: InstrumentId = INSTRUMENT_ID,
        bar_type: BarType = BAR_TYPE,
        fast: int = FAST,
        slow: int = SLOW,
        **_kwargs: object,
    ) -> None:
        super().__init__()
        self.instrument_id = instrument_id
        self.bar_type = bar_type
        self.fast = fast
        self.slow = slow


class EmaCrossAstk(Strategy):
    """EMA(fast) 上穿 EMA(slow) 买入；下穿 平仓。纯多头。"""

    def __init__(self, config: EmaCrossAstkConfig) -> None:
        super().__init__(config)
        self.fast_ema = ExponentialMovingAverage(config.fast)
        self.slow_ema = ExponentialMovingAverage(config.slow)
        self.buys = 0
        self.sells = 0

    def on_start(self) -> None:
        self.register_indicator_for_bars(self.config.bar_type, self.fast_ema)
        self.register_indicator_for_bars(self.config.bar_type, self.slow_ema)
        self.subscribe_bars(self.config.bar_type)

    def on_bar(self, bar: Bar) -> None:
        if not self.indicators_initialized():
            return

        instrument = self.cache.instrument(self.config.instrument_id)
        price = bar.close
        qty = instrument.make_qty(self._lots(price))

        if self.fast_ema.value >= self.slow_ema.value:
            if self.portfolio.is_net_flat(self.config.instrument_id):
                order = self.order_factory.market(
                    self.config.instrument_id,
                    OrderSide.BUY,
                    qty,
                )
                self.submit_order(order)
                self.buys += 1
        else:
            if self.portfolio.is_net_long(self.config.instrument_id):
                self.close_all_positions(self.config.instrument_id)
                self.sells += 1

    def _lots(self, price) -> int:
        """名义额约 60% 现金换算为整数手（lot=100）。"""
        raw = (CASH * 0.6) / float(price)
        lots = max(int(raw // 100), 1)
        return lots * 100

    def on_stop(self) -> None:
        self.close_all_positions(self.config.instrument_id)
        self.log.info(f"EMA done: buys={self.buys} exits={self.sells}")


def main() -> None:
    from nautilus_trader.model import Venue
    from nautilus_trader.persistence import ParquetDataCatalog

    catalog = ParquetDataCatalog("data/astock_catalog")
    instruments = [i for i in catalog.instruments() if str(i.id) == str(INSTRUMENT_ID)]
    if not instruments:
        msg = f"instrument {INSTRUMENT_ID} not in catalog"
        raise SystemExit(msg)
    instrument = instruments[0]
    bars = catalog.query_bars(identifiers=[str(BAR_TYPE)])
    print(f"catalog 就绪: {INSTRUMENT_ID} bars={len(bars)}")

    engine = BacktestEngine(
        config=BacktestEngineConfig(trader_id=TraderId("ASTOCK-001")),
    )
    # 模拟 SH 交易所：现金账户 1000 万 CNY，费率万 2.5（含佣印花近似）
    engine.add_venue(
        venue=Venue("SH"),
        oms_type=OmsType.NETTING,
        account_type=AccountType.CASH,
        base_currency=None,
        starting_balances=[Money(CASH, Currency.from_str("CNY"))],
        fee_model=MakerTakerFeeModel(
            maker_rate=Decimal("0.00025"),
            taker_rate=Decimal("0.00025"),
        ),
    )

    engine.add_instrument(instrument)
    engine.add_data(bars)

    strategy = EmaCrossAstk(config=EmaCrossAstkConfig())
    engine.add_strategy(strategy)
    engine.run()

    # ---- 结果 ----
    fills = engine.generate_order_fills_report()
    print("=" * 62)
    print(f"bars processed : {len(bars)}")
    print(f"buy signals    : {strategy.buys}")
    print(f"exit signals   : {strategy.sells}")
    print(f"order fills    : {len(fills) if fills is not None else 0}")
    acct = engine.generate_account_report(venue=Venue("SH"))
    if acct is not None and len(acct) > 0:
        last = acct.iloc[-1]
        free = getattr(last, "balance_free", None) or getattr(last, "free", None)
        total = getattr(last, "balance_total", None) or getattr(last, "total", None)
        if free is not None and total is not None:
            pnl = float(str(total)) - CASH
            print(f"期末总资产     : {total} CNY（初始 {CASH:,}）")
            print(f"累计盈亏       : {pnl:+,.0f} CNY（{pnl / CASH:+.2%}）")
    print("=" * 62)
    engine.dispose()


if __name__ == "__main__":
    main()
