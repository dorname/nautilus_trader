"""ST 风险筛选器测试（financials 判据 + 行情判据 + 股票池应用 + reporter 阶段聚合）。

本批覆盖用例：UT-STF-01 ~ UT-STF-11、ST-STF-01，共 12 条。
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from nautilus_trader.adapters.astock.financials_client import FinancialMetrics
from nautilus_trader.adapters.astock.financials_client import TickFlowFinancialsClient
from nautilus_trader.adapters.astock.st_filter import AstockSTFilter
from nautilus_trader.adapters.astock.st_filter import STFilterConfig


def _m(period: str, np_: float | None, bps: float | None, **kw) -> FinancialMetrics:
    """构造 FinancialMetrics 辅助函数。"""
    return FinancialMetrics(symbol="X", period_end=period, net_profit=np_, bps=bps, raw=kw)


# ---------------------------------------------------------------------------
# UT-STF-01 ~ UT-STF-07：基本财务/行情判据
# ---------------------------------------------------------------------------


def test_ut_stf_01_negative_equity_hard() -> None:
    """UT-STF-01：净资产为负返回 hard。"""
    f = AstockSTFilter()
    v = f.judge_financials("600001.SH", [_m("2025-12-31", 1e6, -0.5)])
    assert v.excluded and v.severity == "hard"
    assert any("BPS<0" in r for r in v.reasons)


def test_ut_stf_02_consecutive_loss_hard() -> None:
    """UT-STF-02：最近 N 期均净亏损返回 hard。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600002.SH",
        [_m("2024-12-31", -5e7, 2.0), _m("2025-12-31", -3e7, 1.5)],
    )
    assert v.excluded and v.severity == "hard"
    assert any("连续2年净亏损" in r for r in v.reasons)


def test_ut_stf_03_single_loss_warn_kept() -> None:
    """UT-STF-03：单期亏损返回 warn（默认不排除）。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600003.SH",
        [_m("2024-12-31", 5e7, 3.0), _m("2025-12-31", -2e7, 2.8)],
    )
    assert not v.excluded and v.severity == "warn"


def test_ut_stf_04_healthy_clean() -> None:
    """UT-STF-04：健康标的返回 clean。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600004.SH",
        [_m("2024-12-31", 8e8, 5.0), _m("2025-12-31", 9e8, 5.5)],
    )
    assert not v.excluded and v.severity == "clean" and not v.reasons


def test_ut_stf_05_loss_low_revenue_hard() -> None:
    """UT-STF-05：亏损且营收低于 1 亿元返回 hard。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600005.SH",
        [_m("2025-12-31", -1e7, 0.8, revenue=6e7)],
    )
    assert v.excluded and v.severity == "hard"
    assert any("营收<1亿" in r for r in v.reasons)


def test_ut_stf_06_market_price_hard() -> None:
    """UT-STF-06：长期均价低于 2 元返回 hard。"""
    f = AstockSTFilter()
    v = f.judge_market("600006.SH", [1.8] * 30 + [1.9] * 30)
    assert v.excluded and v.severity == "hard"
    assert any("均价" in r for r in v.reasons)


def test_ut_stf_07_market_turnover_warn() -> None:
    """UT-STF-07：日均成交额低于 500 万元返回 warn。"""
    f = AstockSTFilter()
    v = f.judge_market(
        "600007.SH",
        bars_close=[5.0] * 100,
        bars_turnover=[3e6] * 100,  # 300 万 < 500 万阈值
    )
    assert not v.excluded and v.severity == "warn"
    assert any("成交额" in r for r in v.reasons)


# ---------------------------------------------------------------------------
# UT-STF-08：空输入保守排除与汇总跳过判据的区别
# ---------------------------------------------------------------------------


def test_ut_stf_08_empty_inputs_and_missing_source() -> None:
    """UT-STF-08：直接空输入保守排除；screen_pool 单源缺失不伪造另一判据。"""
    f = AstockSTFilter()

    # 直接空输入：保守排除
    v_fin = f.judge_financials("F0.SH", [])
    assert v_fin.excluded and v_fin.severity == "hard"
    assert any("no-financial-data" in r for r in v_fin.reasons)

    v_mkt = f.judge_market("M0.SH", [])
    assert v_mkt.excluded and v_mkt.severity == "hard"
    assert any("no-market-data" in r for r in v_mkt.reasons)

    # screen_pool 无任何源：no-data
    v_none = f.screen_pool(["NODATA.SH"]).get("NODATA.SH")
    assert v_none is not None and v_none.excluded and v_none.severity == "hard"
    assert any("no-data" in r for r in v_none.reasons)

    # 仅有行情源且正常：clean，不能记为财务已检查
    v_bars_only = f.screen_pool(
        ["BARSONLY.SH"],
        catalog_bars={"BARSONLY.SH": [10.0] * 100},
    ).get("BARSONLY.SH")
    assert v_bars_only is not None
    assert not v_bars_only.excluded and v_bars_only.severity == "clean"
    assert not v_bars_only.reasons

    # 仅有财务源且健康：clean，不能记为行情已检查
    v_fin_only = f.screen_pool(
        ["FINONLY.SH"],
        fin_metrics={"FINONLY.SH": [_m("2025-12-31", 1e8, 5.0)]},
    ).get("FINONLY.SH")
    assert v_fin_only is not None
    assert not v_fin_only.excluded and v_fin_only.severity == "clean"
    assert not v_fin_only.reasons


# ---------------------------------------------------------------------------
# UT-STF-09：年报与季报混合，揭示“最近 N 期”不等于连续年度
# ---------------------------------------------------------------------------


def test_ut_stf_09_mixed_annual_quarterly_misleading_reason() -> None:
    """UT-STF-09：年报 + 同年度两份季报均亏损，当前实现仍输出误导性“连续2年净亏损”。"""
    f = AstockSTFilter()
    v = f.judge_financials(
        "600009.SH",
        [
            _m("2024-12-31", -1e8, 2.0),   # 2024 年报
            _m("2025-03-31", -5e7, 1.8),   # 2025 一季报
            _m("2025-06-30", -3e7, 1.5),   # 2025 二季报
        ],
    )
    assert v.excluded and v.severity == "hard"
    # 当前实现按最后 N 期判断，原因文本仍含“连续2年净亏损”——本批不修复业务
    reason_text = "; ".join(v.reasons)
    assert "连续2年净亏损" in reason_text
    assert "2024-12-31" not in reason_text
    assert "2025-03-31" in reason_text
    assert "2025-06-30" in reason_text


# ---------------------------------------------------------------------------
# UT-STF-10：财务解析与缺失字段的实际边界
# ---------------------------------------------------------------------------


def test_ut_stf_10_parse_and_missing_fields() -> None:
    """UT-STF-10：_parse 保留有效字段，缺失字段为 None；无 available_at/revision 属性。"""
    valid = TickFlowFinancialsClient._parse(
        {
            "items": [
                {
                    "symbol": "600519.SH",
                    "period_end": "2026-06-30",
                    "net_profit": 1.2e10,
                    "bps": 158.3,
                },
            ],
        }
    )
    assert len(valid) == 1
    assert valid[0].symbol == "600519.SH"
    assert valid[0].net_profit == 1.2e10
    assert valid[0].bps == 158.3
    assert not valid[0].is_loss

    missing = TickFlowFinancialsClient._parse(
        {
            "items": [
                {"symbol": "MISS.SH", "period_end": "2026-06-30"},
            ],
        }
    )
    assert len(missing) == 1
    m = missing[0]
    assert m.net_profit is None
    assert m.bps is None
    # 当前接口不披露 available_at / revision
    assert not hasattr(m, "available_at")
    assert not hasattr(m, "revision")

    # 缺失关键字段时可能返回 clean 且 reasons 为空，不能证明财务健康
    f = AstockSTFilter()
    v = f.judge_financials("MISS.SH", [m])
    assert not v.excluded and v.severity == "clean"
    assert not v.reasons


# ---------------------------------------------------------------------------
# UT-STF-11：conftest 跨 pytest 阶段状态聚合修正（真实子进程回归）
# ---------------------------------------------------------------------------


def test_ut_stf_11_reporter_stage_aggregation(tmp_path: Path) -> None:
    """UT-STF-11：真实 pytest 子进程验证 setup/call/teardown 状态聚合。"""
    conftest_src = Path(__file__).with_name("conftest.py").read_text(encoding="utf-8")
    # 子进程使用临时输出路径，不污染仓库真实 JSONL
    results_path = tmp_path / "sub-results.jsonl"
    env_override = f'os.environ["OPENLOGOS_RESULTS_PATH"] = r"{results_path}"\n'
    # 把环境变量覆盖插到 conftest 的 _RESULTS 定义之后即可
    conftest_body = conftest_src.replace(
        "_RESULTS = Path(os.environ.get(",
        env_override + "_RESULTS = Path(os.environ.get(",
    )
    (tmp_path / "conftest.py").write_text(conftest_body, encoding="utf-8")

    test_code = '''
import pytest

@pytest.fixture
def ok_fixture():
    return 1

@pytest.fixture
def setup_fail_fixture():
    raise RuntimeError("setup failure")

@pytest.fixture
def teardown_fail_fixture():
    yield 1
    raise RuntimeError("teardown failure")

def test_ut_sub_01_pass(ok_fixture):
    assert ok_fixture == 1

def test_ut_sub_02_call_fail(ok_fixture):
    assert ok_fixture == 2

@pytest.mark.skip(reason="intentional skip")
def test_ut_sub_03_skip(ok_fixture):
    assert False

def test_ut_sub_04_setup_fail(setup_fail_fixture):
    assert True

def test_ut_sub_05_teardown_fail(teardown_fail_fixture):
    assert True
'''
    (tmp_path / "test_sub.py").write_text(test_code, encoding="utf-8")

    # 用与 run-astock-tests.sh 一致的解释器发现逻辑
    py_exe = sys.executable
    cmd = [py_exe, "-m", "pytest", str(tmp_path), "-q", "--tb=short"]
    env = os.environ.copy()
    env["OPENLOGOS_RESULTS_PATH"] = str(results_path)
    env["PYTHONPATH"] = str(Path(__file__).resolve().parents[3]) + (f":{env['PYTHONPATH']}" if env.get("PYTHONPATH") else "")
    proc = subprocess.run(cmd, capture_output=True, text=True, env=env)
    # 允许子进程测试自身失败，我们断言的是 JSONL 记录
    assert results_path.exists(), f"子进程未生成 JSONL：\n{proc.stdout}\n{proc.stderr}"

    records: list[dict] = []
    for line in results_path.read_text(encoding="utf-8").strip().splitlines():
        records.append(json.loads(line))

    by_id = {r["id"]: r for r in records}
    # 子进程用例 ID 命名约定生成 UT-SUB-01 ~ UT-SUB-05
    assert by_id["UT-SUB-01"]["status"] == "pass"
    assert "error" not in by_id["UT-SUB-01"]

    assert by_id["UT-SUB-02"]["status"] == "fail"
    assert "error" in by_id["UT-SUB-02"]

    assert by_id["UT-SUB-03"]["status"] == "skip"
    assert "error" not in by_id["UT-SUB-03"]

    assert by_id["UT-SUB-04"]["status"] == "fail"
    assert "error" in by_id["UT-SUB-04"]

    assert by_id["UT-SUB-05"]["status"] == "fail"
    assert "error" in by_id["UT-SUB-05"]

    # duration 与 timestamp 存在且 duration 非负
    for r in records:
        assert "duration_ms" in r and isinstance(r["duration_ms"], int) and r["duration_ms"] >= 0
        assert "timestamp" in r


# ---------------------------------------------------------------------------
# ST-STF-01：screen_pool → apply_pool 内部 API 编排
# ---------------------------------------------------------------------------


def test_st_stf_01_screen_pool_apply_pool() -> None:
    """ST-STF-01：按编排规格验证 screen_pool → apply_pool 默认/严格池。"""
    f = AstockSTFilter(STFilterConfig())
    fin_metrics = {
        "GOOD.SH": [
            _m("2024-12-31", 8e8, 5.0),
            _m("2025-12-31", 9e8, 5.5),
        ],
        "WARN.SH": [
            _m("2024-12-31", 5e7, 3.0),
            _m("2025-12-31", -2e7, 2.8),
        ],
        "LOSS.SH": [
            _m("2024-12-31", -5e7, 2.0),
            _m("2025-12-31", -3e7, 1.5),
        ],
    }
    catalog_bars = {
        "GOOD.SH": [10.0],
        "WARN.SH": [10.0],
        "LOSS.SH": [10.0],
    }
    catalog_turnovers = {
        "GOOD.SH": [1e7],
        "WARN.SH": [1e7],
        "LOSS.SH": [1e7],
    }

    verdicts = f.screen_pool(
        ["GOOD.SH", "WARN.SH", "LOSS.SH", "NODATA.SH"],
        fin_metrics=fin_metrics,
        catalog_bars=catalog_bars,
        catalog_turnovers=catalog_turnovers,
    )

    assert verdicts["GOOD.SH"].severity == "clean"
    assert not verdicts["GOOD.SH"].excluded
    assert verdicts["WARN.SH"].severity == "warn"
    assert not verdicts["WARN.SH"].excluded
    assert verdicts["LOSS.SH"].severity == "hard"
    assert verdicts["LOSS.SH"].excluded
    assert verdicts["NODATA.SH"].severity == "hard"
    assert verdicts["NODATA.SH"].excluded
    assert any("no-data" in r for r in verdicts["NODATA.SH"].reasons)

    default_pool = AstockSTFilter.apply_pool(verdicts, drop_warn=False)
    strict_pool = AstockSTFilter.apply_pool(verdicts, drop_warn=True)

    assert set(default_pool) == {"GOOD.SH", "WARN.SH"}
    assert strict_pool == ["GOOD.SH"]


if __name__ == "__main__":
    # 最小本地回退：不经过 pytest，仅用于快速手工触发
    fails = 0
    for name, fn in sorted({k: v for k, v in globals().items() if k.startswith("test_")}.items()):
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            fails += 1
            print(f"FAIL {name}: {exc}")
    sys.exit(1 if fails else 0)
