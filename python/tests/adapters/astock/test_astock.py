"""astock 数据加载层单元测试（UT-AST-01~11 的可自动化子集）。

运行：python3 -m pytest python/tests/adapters/astock/ -q
（无 pytest 时：python3 python/tests/adapters/astock/test_astock.py）
"""

from __future__ import annotations

import json
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


def test_ut_ast_05_06_07_merge_dedup_priority() -> None:
    """UT-AST-05/06/07：ts_event 去重 + TickFlow 冲突优先 + BarType 规范。"""
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


def test_ut_ast_09_incremental_start_time() -> None:
    """UT-AST-09：catalog 已有至 T 日 → 增量请求 start_time=T+1，不重拉历史。"""
    from unittest.mock import patch

    client = TickFlowClient(api_key="k-test")  # noqa: S106
    t_close_ms = 1756684800 * 1000  # 2025-09-01 15:00 CST
    t_next_ms = t_close_ms + 86_400_000  # T+1 同时刻
    captured: dict = {}

    def fake_request(url: str) -> dict:
        captured["url"] = url
        return {"items": []}

    with patch.object(client, "_request_json", side_effect=fake_request):
        out = client.fetch_daily("600000.SH", start_time_ms=t_next_ms)
    assert out == []
    assert "start_time" in captured["url"]
    assert str(t_next_ms) in captured["url"]
    # 未传 start_time 时不得携带该参数（全量语义）
    with patch.object(client, "_request_json", side_effect=fake_request):
        client.fetch_daily("600000.SH", count=10)
    assert "start_time" not in captured["url"]


def test_ut_ast_10_api_key_not_leaked() -> None:
    """UT-AST-10：全链路（异常消息/URL/日志）无明文 API Key。"""
    import io
    import logging
    import urllib.error
    from unittest.mock import patch

    secret = "sk-SECRET-LEAK-PROBE-7f3a"
    client = TickFlowClient(api_key=secret)  # noqa: S106
    log_buf = io.StringIO()
    handler = logging.StreamHandler(log_buf)
    logger = logging.getLogger("nautilus_trader.adapters.astock")
    logger.addHandler(handler)
    logger.setLevel(logging.DEBUG)

    captured: list[Exception] = []

    def fake_urlopen(request, timeout):  # noqa: ANN001, ANN202
        # key 只应经请求头传递（不进 URL）
        assert request.get_header("X-api-key") == secret, "key 必须经请求头传递"
        assert secret not in request.full_url, "URL 不得携带 key"
        raise urllib.error.HTTPError(request.full_url, 401, "unauthorized", None, None)

    with patch("urllib.request.urlopen", side_effect=fake_urlopen):
        try:
            client.fetch_daily("600000.SH")
        except Exception as exc:  # noqa: BLE001
            captured.append(exc)
    assert captured, "401 必须抛错"
    assert secret not in str(captured[0]), "异常消息不得含明文 key"
    assert secret not in log_buf.getvalue(), "日志不得含明文 key"
    logger.removeHandler(handler)


def test_st_a1_tdx_full_build() -> None:
    """ST-A1：通达信样例目录全量建库（真实 examples 数据端到端）。"""
    import tempfile

    root = Path(__file__).resolve().parents[4]
    tdx_dir = root / "examples" / "tongxindadata" / "lday"
    assert tdx_dir.is_dir(), "样例目录必须存在"
    records: list = []
    warnings = 0
    for _path, payload in scan_tdx_dir(tdx_dir):
        if isinstance(payload, str):
            warnings += 1
            continue
        records.extend(payload)
    assert len(records) > 10_000, "样例应有大量有效记录"
    assert warnings == 0, "样例目录不应有解析告警"

    with tempfile.TemporaryDirectory() as tmp:
        writer = AstockCatalogWriter(str(Path(tmp) / "catalog"))
        n = writer.add_tdx(records)
        assert n == len(records), "全部记录入库"
        merged = writer.merged_bars()
        assert len(merged) == n, "合并不得丢失记录（样例单源无重叠）"
        bars, conflicts = writer.merge_and_write(nautilus=False)
        assert conflicts == 0 and len(bars) == len(merged)
        out = (Path(tmp) / "catalog").with_suffix(".jsonl")
        lines = out.read_text(encoding="utf-8").strip().splitlines()
        assert len(lines) == len(merged), "产物行数与合并条数一致"
        first = json.loads(lines[0])
        assert first["instrument_id"].endswith((".SH", ".SZ"))
        assert first["bar_type"].endswith("-1-DAY-LAST-EXTERNAL")


def test_st_a2_dual_source_union() -> None:
    """ST-A2：tdx 全量 + tickflow 增量 → 重叠按 FR-A4 收敛，总条数=并集。"""
    import datetime as dt

    cst = dt.timezone(dt.timedelta(hours=8))

    def close_ms(y: int, m: int, d: int) -> int:
        return int(dt.datetime(y, m, d, 15, 0, tzinfo=cst).timestamp() * 1000)

    # tdx 全量：三日
    tdx_rows = [
        TdxDayRecord(20250901, 10.5, 11.0, 10.4, 10.8, 1.2e8, 900_000),
        TdxDayRecord(20250902, 10.8, 11.2, 10.7, 11.1, 1.1e8, 880_000),
        TdxDayRecord(20250903, 11.1, 11.5, 11.0, 11.3, 1.0e8, 860_000),
    ]
    # tickflow 增量：仅 09-02/09-03（与 tdx 重叠一天 + 新增一天之外再补 09-04）
    tf_klines = [
        TickFlowKline("600000.SH", close_ms(2025, 9, 2), 10.8, 11.2, 10.7, 11.15, 880_000),
        TickFlowKline("600000.SH", close_ms(2025, 9, 3), 11.1, 11.5, 11.0, 11.35, 860_000),
        TickFlowKline("600000.SH", close_ms(2025, 9, 4), 11.35, 11.8, 11.3, 11.7, 900_000),
    ]
    writer = AstockCatalogWriter("/tmp/astock-st-a2")
    writer.add_tdx([("600000", "SH", r) for r in tdx_rows])
    writer.add_tickflow("600000.SH", tf_klines)
    merged = writer.merged_bars("600000.SH")
    assert len(merged) == 4, "并集：3 天 tdx ∪ 3 天 tickflow，重叠 2 天 → 4 条"
    by_date = {b.ts_event_ns: b for b in merged}
    # 重叠日以 TickFlow 为准（FR-A4）
    d3 = by_date[close_ms(2025, 9, 3) * 1_000_000]
    assert d3.source == "tickflow" and d3.close == 11.35
    # 仅 tdx 的日子保留 tdx 值
    d1 = by_date[close_ms(2025, 9, 1) * 1_000_000]
    assert d1.source == "tdx" and d1.close == 10.8


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
