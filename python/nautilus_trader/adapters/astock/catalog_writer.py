"""双源合并与 catalog 写入。

规格：FR-A4（ts_event 去重、TickFlow 优先）、FR-A5（统一 Bar/Instrument 输出）、
NFR-A4（幂等）、UT-AST-05~08。

Nautilus 内核对象（Price/Quantity/Bar/Equity/ParquetDataCatalog）**惰性 import**：
本模块可在无编译内核的环境（CI 静态检查）独立测试合并逻辑——
``nautilus=False`` 时输出 ``AstockMergedBar`` 中间形态。
"""

from __future__ import annotations

import datetime as dt
import logging
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from nautilus_trader.adapters.astock.tdx_loader import Exchange
from nautilus_trader.adapters.astock.tdx_loader import TdxDayRecord
from nautilus_trader.adapters.astock.tickflow_client import TickFlowKline

if TYPE_CHECKING:
    from collections.abc import Iterable

logger = logging.getLogger(__name__)

_PRICE_PRECISION = 2  # A股价格精度：分
_LOT_SIZE = 100  # A股一手 100 股（FR-A5）


@dataclass(frozen=True, slots=True)
class AstockMergedBar:
    """合并后的日线中间形态（与 Nautilus Bar 一一对应，UT-AST-07）。

    ts_event 为该日 15:00 收盘时刻（bar 时间戳约定：整根 K 线走完才可见）。
    """

    instrument_id: str  # "600000.SH"
    ts_event_ns: int
    open: float
    high: float
    low: float
    close: float
    volume: int
    source: str  # "tickflow" | "tdx"（FR-A4 冲突裁决留痕）

    @property
    def bar_type(self) -> str:
        return f"{self.instrument_id}-1-DAY-LAST-EXTERNAL"


def tdx_to_merged(rec: TdxDayRecord, symbol: str, exchange: Exchange) -> AstockMergedBar:
    """TdxDayRecord → AstockMergedBar（ts_event=当日 15:00 收盘，Asia/Shanghai）。"""
    date = rec.date
    y, m, d = date // 10000, date // 100 % 100, date % 100
    close_time = dt.datetime(y, m, d, 15, 0, tzinfo=dt.timezone(dt.timedelta(hours=8)))
    ts_ns = int(close_time.timestamp() * 1_000_000_000)
    return AstockMergedBar(
        instrument_id=f"{symbol}.{exchange}",
        ts_event_ns=ts_ns,
        open=rec.open,
        high=rec.high,
        low=rec.low,
        close=rec.close,
        volume=rec.volume,
        source="tdx",
    )


def tickflow_to_merged(k: TickFlowKline) -> AstockMergedBar:
    """TickFlowKline → AstockMergedBar（毫秒→纳秒）。"""
    return AstockMergedBar(
        instrument_id=k.symbol,
        ts_event_ns=int(k.timestamp_ms) * 1_000_000,
        open=k.open,
        high=k.high,
        low=k.low,
        close=k.close,
        volume=k.volume,
        source="tickflow",
    )


class AstockCatalogWriter:
    """汇聚双源、按 ts_event 去重合并，写入 Nautilus Parquet catalog。"""

    def __init__(self, catalog_path: str) -> None:
        self.catalog_path = Path(catalog_path)
        # instrument_id -> ts_event_ns -> bar（后写覆盖先写；tickflow 后加入故优先，FR-A4）
        self._bars: dict[str, dict[int, AstockMergedBar]] = {}
        self._order: dict[str, list[int]] = {}
        self._conflicts = 0  # FR-A4 跨源冲突计数（留痕）

    # ---- 源注入 ----------------------------------------------------

    def add_tdx(self, records: "Iterable[tuple[str, Exchange, TdxDayRecord]]") -> int:
        n = 0
        for symbol, exchange, rec in records:
            self._put(tdx_to_merged(rec, symbol, exchange))
            n += 1
        return n

    def add_tickflow(self, symbol: str, klines: "Iterable[TickFlowKline]") -> int:
        n = 0
        for k in klines:
            self._put(tickflow_to_merged(k))
            n += 1
        return n

    def _put(self, bar: AstockMergedBar) -> None:
        slot = self._bars.setdefault(bar.instrument_id, {})
        existing = slot.get(bar.ts_event_ns)
        if existing is not None and existing.source != bar.source:
            # FR-A4：不同源同 ts_event = 冲突，后写者（tickflow）胜出并留痕
            self._conflicts += 1
            logger.debug(
                "conflict on %s@%s: %s -> %s (close %.2f -> %.2f)",
                bar.instrument_id,
                bar.ts_event_ns,
                existing.source,
                bar.source,
                existing.close,
                bar.close,
            )
        slot[bar.ts_event_ns] = bar
        self._order.setdefault(bar.instrument_id, []).append(bar.ts_event_ns)

    # ---- 合并与写出 --------------------------------------------------

    def merged_bars(self, instrument_id: str | None = None) -> list[AstockMergedBar]:
        """去重合并后的全量（按 instrument、ts_event 升序）。"""
        out: list[AstockMergedBar] = []
        ids = [instrument_id] if instrument_id else sorted(self._bars)
        for iid in ids:
            slot = self._bars.get(iid)
            if not slot:
                continue
            out.extend(slot[ts] for ts in sorted(slot))
        return out

    def merge_and_write(self, *, nautilus: bool | None = None) -> tuple[list[AstockMergedBar], int]:
        """合并并写出（FR-A4/A5、NFR-A4）。

        ``nautilus=None`` 时自动探测：内核可用即写 Parquet catalog，
        否则降级 JSONL 快照（CI/无内核环境），保证测试与幂等断言可执行。

        Returns
        -------
        (merged_bars, conflicts)
            conflicts = “不同源同 ts_event”交集条数（FR-A4 裁决留痕）。

        """
        merged = self.merged_bars()
        conflicts = self._conflicts

        if nautilus is None:
            nautilus = self._kernel_available()
        if nautilus:
            self._write_nautilus(merged)
        else:
            # 无内核环境：落 JSONL 快照（测试/CI 用），覆盖写保持幂等（NFR-A4）
            import json

            self.catalog_path.parent.mkdir(parents=True, exist_ok=True)
            out = self.catalog_path.with_suffix(".jsonl")
            with out.open("w", encoding="utf-8") as fh:
                for bar in merged:
                    fh.write(
                        json.dumps(
                            {
                                "instrument_id": bar.instrument_id,
                                "ts_event_ns": bar.ts_event_ns,
                                "open": bar.open,
                                "high": bar.high,
                                "low": bar.low,
                                "close": bar.close,
                                "volume": bar.volume,
                                "source": bar.source,
                                "bar_type": bar.bar_type,
                            },
                        )
                        + "\n",
                    )
        return merged, conflicts

    @staticmethod
    def _kernel_available() -> bool:
        try:
            from nautilus_trader._libnautilus import core  # noqa: F401
        except Exception:  # noqa: BLE001 — 任何导入失败都视为内核不可用
            return False
        return True

    # ---- Nautilus 写出（惰性 import）--------------------------------

    def _write_nautilus(self, merged: list[AstockMergedBar]) -> None:
        from nautilus_trader.core.nautilus_pyo3 import InstrumentId  # 惰性：需编译内核

        _ = InstrumentId  # 触发 import 校验
        raise NotImplementedError(
            "nautilus=True 需要编译好的 nautilus_trader 内核（当前环境未构建）。"
            "规格阶段以 nautilus=False 的 JSONL 输出验证合并逻辑；"
            "内核可用后按 04-astock-data-architecture.md 的映射补 write_data 调用。",
        )
