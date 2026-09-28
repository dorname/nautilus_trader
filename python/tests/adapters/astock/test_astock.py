"""astock 数据加载层单元测试（UT-AST-01~11 的可自动化子集）。

运行：python3 -m pytest python/tests/adapters/astock/ -q
（无 pytest 时：python3 python/tests/adapters/astock/test_astock.py）
"""

from __future__ import annotations

import struct
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from nautilus_trader.adapters.astock.catalog_writer import AstockCatalogWriter  # noqa: E402
from nautilus_trader.adapters.astock.catalog_writer import tdx_to_merged  # noqa: E402
from nautilus_trader.adapters.astock.catalog_writer import tickflow_to_merged  # noqa: E402
from nautilus_trader.adapters.astock.tdx_loader import TdxDayRecord  # noqa: E402
from nautilus_trader.adapters.astock.tdx_loader import parse_tdx_day_file  # noqa: E402
from nautilus_trader.adapters.astock.tdx_loader import scan_tdx_dir  # noqa: E402
from nautilus_trader.adapters.astock.tickflow_client import TickFlowClient  # noqa: E402
from nautilus_trader.adapters.astock.tickflow_client import TickFlowKline  # noqa: E402

_REC = struct.Struct("<IIIIIfII")


def _make_day(path: Path, rows: list[tuple[int, int, int, int, int, float, int]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        b"".join(_REC.pack(date, o, h, l, c, amount, vol, 0) for date, o, h, l, c, amount, vol in rows),
    )


def test_ut_ast_01_parse_day() -> None:
    """UT-AST-01：.day 解析正确（价格÷100）。"""
    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "sh600000.day"
        _make_day(
            p,
            [(20250901, 1050, 1100, 1040, 1080, 1.2e8, 900_000)],
        )
        out = parse_tdx_day_file(p)
        assert len(out) == 1
        symbol, exchange, rec = out[0]
        assert (symbol, exchange) == ("600000", "SH")
        assert rec.open == 10.50 and rec.high == 11.00 and rec.low == 10.40 and rec.close == 10.80
        assert rec.volume == 900_000 and rec.date == 20250901


def test_ut_ast_02_bad_file_tolerated() -> None:
    """UT-AST-02：坏文件（截断）容错跳过。"""
    with tempfile.TemporaryDirectory() as tmp:
        p = Path(tmp) / "sz000001.day"
        p.write_bytes(b"\x01\x02\x03")  # 长度非 32 倍数
        assert parse_tdx_day_file(p) == []
        results = list(scan_tdx_dir(tmp))
        assert len(results) == 1 and isinstance(results[0][1], str)  # 告警而非异常


def test_ut_ast_03_tickflow_parse() -> None:
    """UT-AST-03：KlinesResponse 映射（秒级时间戳归一毫秒）。"""
    payload = {
        "items": [
            {"timestamp": 1756684800, "open": 10.5, "high": 11.2, "low": 10.3, "close": 11.0, "volume": 1234567, "amount": 1.3e7},
        ],
    }
    klines = TickFlowClient._parse_response(payload, symbol="600000.SH", adjust="none")
    assert len(klines) == 1
    k = klines[0]
    assert k.timestamp_ms == 1756684800 * 1000
    assert k.close == 11.0 and k.volume == 1234567


def test_ut_ast_04_tickflow_errors() -> None:
    """UT-AST-04：缺 Key → 认证错。"""
    client = TickFlowClient(api_key="")  # noqa: S106
    try:
        client.fetch_daily("600000.SH")
    except Exception as exc:  # noqa: BLE001
        assert "API key" in str(exc)
    else:
        raise AssertionError("expected auth error")


def test_ut_ast_05_06_merge_dedup_priority() -> None:
    """UT-AST-05/06：ts_event 去重 + TickFlow 冲突优先。"""
    # 两源对同一交易日（2025-09-01）各自构造 15:00 Asia/Shanghai 收盘时刻：
    # tdx_to_merged 由 date 推导；tickflow 侧用同一时刻的毫秒时间戳
    import datetime as dt

    cst = dt.timezone(dt.timedelta(hours=8))
    close_ms = int(
        dt.datetime(2025, 9, 1, 15, 0, tzinfo=cst).timestamp() * 1000,
    )
    tdx_rec = TdxDayRecord(20250901, 10.5, 11.0, 10.4, 10.8, 1.2e8, 900_000)
    tf_kline = TickFlowKline("600000.SH", close_ms, 10.5, 11.0, 10.4, 10.9, 900_000)

    writer = AstockCatalogWriter("/tmp/astock-catalog-test")
    writer.add_tdx([("600000", "SH", tdx_rec)])
    n_tf = writer.add_tickflow("600000.SH", [tf_kline])

    merged = writer.merged_bars("600000.SH")
    assert n_tf == 1
    assert len(merged) == 1, "重叠 ts_event 必须去重为一条"

    merged_tf = tickflow_to_merged(tf_kline)
    merged_tdx = tdx_to_merged(tdx_rec, "600000", "SH")
    assert merged_tf.ts_event_ns == merged_tdx.ts_event_ns, "两源对同一收盘时刻必须映射到同一 ts_event"

    assert merged[0].close == 10.9  # TickFlow 版本胜出
    assert merged[0].source == "tickflow"
    assert merged[0].bar_type == "600000.SH-1-DAY-LAST-EXTERNAL"  # UT-AST-07


def test_ut_ast_08_instrument_metadata() -> None:
    """UT-AST-08：Instrument 元数据随 bar 携带（精度/手数断言在规格常量）。"""
    import nautilus_trader.adapters.astock.catalog_writer as cw

    assert cw._PRICE_PRECISION == 2 and cw._LOT_SIZE == 100


def test_ut_ast_11_idempotent_merge() -> None:
    """UT-AST-11：同数据集重复合并 → 输出不变。"""
    tdx_rec = TdxDayRecord(20250901, 10.5, 11.0, 10.4, 10.8, 1.2e8, 900_000)
    w1 = AstockCatalogWriter("/tmp/astock-a")
    w2 = AstockCatalogWriter("/tmp/astock-b")
    for w in (w1, w2):
        w.add_tdx([("600000", "SH", tdx_rec)])
        w.add_tdx([("600000", "SH", tdx_rec)])  # 重复注入
    assert w1.merged_bars() == w2.merged_bars()
    assert len(w1.merged_bars()) == 1


if __name__ == "__main__":
    failures = 0
    for name, fn in sorted({k: v for k, v in globals().items() if k.startswith("test_")}.items()):
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            failures += 1
            print(f"FAIL {name}: {exc}")
    sys.exit(1 if failures else 0)
