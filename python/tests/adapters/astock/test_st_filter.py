"""ST 风险筛选器测试（financials 判据 + 行情判据 + 股票池应用）。"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from nautilus_trader.adapters.astock.financials_client import FinancialMetrics
from nautilus_trader.adapters.astock.st_filter import AstockSTFilter
from nautilus_trader.adapters.astock.st_filter import STFilterConfig


def _m(period: str, np_: float | None, bps: float | None, **kw) -> FinancialMetrics:
    return FinancialMetrics(symbol="X", period_end=period, net_profit=np_, bps=bps, raw=kw)


def test_negative_equity_hard() -> None:
    """规则2：净资产为负 → hard 剔除。"""
    f = AstockSTFilter()
    v = f.judge_financials("600001.SH", [_m("2025-12-31", 1e6, -0.5)])
    assert v.excluded and v.severity == "hard"
    assert any("BPS<0" in r for r in v.reasons)


def test_consecutive_loss_hard() -> None:
    """规则4：连续两年亏损 → hard。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600002.SH",
        [_m("2024-12-31", -5e7, 2.0), _m("2025-12-31", -3e7, 1.5)],
    )
    assert v.excluded and v.severity == "hard"
    assert any("连续2年净亏损" in r for r in v.reasons)


def test_single_loss_warn_kept() -> None:
    """单年亏损 → warn 保留（默认不剔除 warn）。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600003.SH",
        [_m("2024-12-31", 5e7, 3.0), _m("2025-12-31", -2e7, 2.8)],
    )
    assert not v.excluded and v.severity == "warn"


def test_healthy_clean() -> None:
    """健康标的 → clean。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600004.SH",
        [_m("2024-12-31", 8e8, 5.0), _m("2025-12-31", 9e8, 5.5)],
    )
    assert not v.excluded and v.severity == "clean" and not v.reasons


def test_loss_low_revenue_hard() -> None:
    """规则1：亏损 + 营收<1亿 → hard。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600005.SH",
        [_m("2025-12-31", -1e7, 0.8, revenue=6e7)],
    )
    assert v.excluded and v.severity == "hard"
    assert any("营收<1亿" in r for r in v.reasons)


def test_market_price_hard() -> None:
    """规则7：长期均价 < 2 元 → hard（面值退市前瞻）。"""
    f = AstockSTFilter()
    v = f.judge_market("600006.SH", [1.8] * 30 + [1.9] * 30)
    assert v.excluded and any("均价" in r for r in v.reasons)


def test_market_turnover_warn() -> None:
    """规则6：流动性枯竭 → warn。"""
    f = AstockSTFilter()
    v = f.judge_market(
        "600007.SH",
        bars_close=[5.0] * 100,
        bars_turnover=[3e6] * 100,  # 300万 < 500万阈值
    )
    assert not v.excluded and v.severity == "warn"
    assert any("成交额" in r for r in v.reasons)


def test_no_data_hard() -> None:
    """无任何数据 → 剔除（保守）。"""
    f = AstockSTFilter()
    v = f.screen_pool(["600008.SH"]).get("600008.SH")
    assert v is not None and v.excluded


def test_pool_apply_end_to_end() -> None:
    """端到端：混合股票池 → 剔除 hard，保留 clean/warn。"""
    f = AstockSTFilter(STFilterConfig())
    fin = {
        "GOOD.SH": [_m("2025-12-31", 1e9, 6.0)],
        "LOSS.SH": [_m("2024-12-31", -5e7, 2.0), _m("2025-12-31", -3e7, 1.5)],
        "NEG.SH": [_m("2025-12-31", 1e6, -0.5)],
    }
    bars = {"GOOD.SH": [10.0] * 100, "LOSS.SH": [8.0] * 100, "NEG.SH": [3.0] * 100}
    verdicts = f.screen_pool(list(fin), fin_metrics=fin, catalog_bars=bars)
    pool = AstockSTFilter.apply_pool(verdicts)
    assert "GOOD.SH" in pool
    assert "LOSS.SH" not in pool
    assert "NEG.SH" not in pool


def test_financials_parse() -> None:
    """财务接口响应解析（宽松字段名）。"""
    payload = {"items": [{"symbol": "600519.SH", "period_end": "2026-06-30", "net_profit": 1.2e10, "bps": 158.3}]}
    out = TickFlowFinancialsClient._parse(payload)
    assert len(out) == 1
    assert out[0].symbol == "600519.SH" and out[0].bps == 158.3
    assert not out[0].is_loss


from nautilus_trader.adapters.astock.financials_client import TickFlowFinancialsClient  # noqa: E402


if __name__ == "__main__":
    fails = 0
    for name, fn in sorted({k: v for k, v in globals().items() if k.startswith("test_")}.items()):
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            fails += 1
            print(f"FAIL {name}: {exc}")
    sys.exit(1 if fails else 0)
