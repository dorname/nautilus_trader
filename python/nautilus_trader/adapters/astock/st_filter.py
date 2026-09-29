"""A股 ST 风险筛选器 — 从股票池中排除可能被 ST 的标的。

ST 规则依据（上交所/深交所《股票上市规则》退市风险警示核心条款）：
  *ST（退市风险警示）真实触发条件：
  1. 最近一个会计年度经审计净利润为负且营业收入低于 1 亿元（组合指标）
  2. 最近一个会计年度期末净资产为负
  3. 财务报告被出具无法表示意见/否定意见审计报告
  ST（其他风险警示）常见触发：
  4. 连续两年净利润为负（旧规则主通道，仍是强前瞻信号）
  5. 生产经营活动受严重影响/主要银行账号被冻结等（非财务，无法量化）

可量化部分 = 1/2/4（本筛选器覆盖）；3/5 需公告文本，超数据能力，明确声明不覆盖。
行情侧补充信号（TDX 日线可算，无需财务接口）：
  6. 流动性枯竭：长期日均成交额 < 500 万（ST 前兆）
  7. 价格面值退市风险：收盘价长期 < 2 元（退市红线 1 元的前瞻缓冲）

数据源：
  - 财务：TickFlowFinancialsClient（/v1/financials/metrics，多期）
  - 行情：本地 Parquet catalog（data/astock_catalog）
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from nautilus_trader.adapters.astock.financials_client import FinancialMetrics
from nautilus_trader.adapters.astock.financials_client import TickFlowFinancialsClient

if TYPE_CHECKING:
    from collections.abc import Iterable

logger = logging.getLogger(__name__)


@dataclass(frozen=True, slots=True)
class STRiskVerdict:
    """单只股票的 ST 风险判定。"""

    symbol: str
    excluded: bool
    reasons: tuple[str, ...]
    severity: str  # "hard"（已触发判据）/ "warn"（接近阈值）/ "clean"

    @property
    def as_line(self) -> str:
        return f"{self.symbol}\t{self.severity}\t{'; '.join(self.reasons) or '-'}"


@dataclass(frozen=True, slots=True)
class STFilterConfig:
    """可调阈值（默认按交易所规则 + 经验缓冲）。"""

    min_revenue_cny: float = 1.0e8  # 规则1：营收 < 1 亿（与净亏损组合）
    min_bps: float = 0.0  # 规则2：每股净资产 > 0
    consecutive_loss_years: int = 2  # 规则4：连续 N 年净亏损 → hard
    loss_warn_years: int = 1  # 1 年亏损（未连亏）→ warn
    min_avg_turnover_cny: float = 5.0e6  # 规则6：日均成交额下限
    min_price: float = 2.0  # 规则7：价格下限（1 元退市线缓冲）
    turnover_window_days: int = 60  # 流动性观察窗口


class AstockSTFilter:
    """股票池 ST 风险筛选器：财务判据 + 行情判据。"""

    def __init__(
        self,
        config: STFilterConfig | None = None,
        financials: TickFlowFinancialsClient | None = None,
    ) -> None:
        self.cfg = config or STFilterConfig()
        self._fin = financials

    # ---- 财务判据（规则 1/2/4）------------------------------------

    def judge_financials(self, symbol: str, metrics: list[FinancialMetrics]) -> STRiskVerdict:
        """按报告期升序的财务指标判定。metrics 需 ≥1 期，建议 3 期。"""
        reasons: list[str] = []
        severity = "clean"

        if not metrics:
            return STRiskVerdict(symbol, True, ("no-financial-data",), "hard")

        mets = sorted(metrics, key=lambda m: m.period_end)
        latest = mets[-1]

        # 规则2：净资产为负 → hard（*ST 直接判据）
        if latest.negative_equity:
            reasons.append(f"BPS<0 ({latest.bps:.2f} @ {latest.period_end})")
            severity = "hard"

        # 规则4：连续亏损 → hard / 单年亏损 → warn
        losses = [m for m in mets[-self.cfg.consecutive_loss_years :]]
        loss_marks = [m.is_loss for m in losses]
        if len(loss_marks) >= self.cfg.consecutive_loss_years and all(loss_marks):
            years = ", ".join(m.period_end for m in losses)
            reasons.append(f"连续{self.cfg.consecutive_loss_years}年净亏损 [{years}]")
            severity = "hard"
        elif latest.is_loss:
            reasons.append(f"最近一期亏损 (NP={latest.net_profit:.3g} @ {latest.period_end})")
            severity = "warn" if severity == "clean" else severity

        # 规则1（近似）：亏损 + 营收规模小（无营收字段时退化为亏损即可警示）
        rev = latest.raw.get("revenue") if latest.raw else None
        if latest.is_loss and rev is not None:
            try:
                if float(rev) < self.cfg.min_revenue_cny:
                    reasons.append(f"亏损且营收<1亿 ({float(rev):.3g})")
                    severity = "hard"
            except (TypeError, ValueError):
                pass

        excluded = severity == "hard"
        return STRiskVerdict(symbol, excluded, tuple(reasons), severity)

    # ---- 行情判据（规则 6/7，本地 catalog，无需 API）----------------

    def judge_market(
        self,
        symbol: str,
        bars_close: "Iterable[float]",
        bars_turnover: "Iterable[float] | None" = None,
    ) -> STRiskVerdict:
        closes = list(bars_close)
        reasons: list[str] = []
        severity = "clean"

        if not closes:
            return STRiskVerdict(symbol, True, ("no-market-data",), "hard")

        recent = closes[-self.cfg.turnover_window_days :]
        avg_price = sum(recent) / len(recent)
        if avg_price < self.cfg.min_price:
            reasons.append(f"均价<{self.cfg.min_price}元 ({avg_price:.2f})")
            severity = "hard"  # 面值退市前瞻

        turns = list(bars_turnover) if bars_turnover is not None else None
        if turns:
            recent_t = turns[-self.cfg.turnover_window_days :]
            avg_to = sum(recent_t) / len(recent_t)
            if avg_to < self.cfg.min_avg_turnover_cny:
                reasons.append(f"日均成交额<{self.cfg.min_avg_turnover_cny/1e6:.0f}M ({avg_to/1e6:.1f}M)")
                severity = "warn" if severity == "clean" else severity

        return STRiskVerdict(symbol, severity == "hard", tuple(reasons), severity)

    # ---- 汇总 ----------------------------------------------------

    def screen_pool(
        self,
        symbols: list[str],
        *,
        catalog_bars: "dict[str, list[float]] | None" = None,
        catalog_turnovers: "dict[str, list[float]] | None" = None,
        fin_metrics: "dict[str, list[FinancialMetrics]] | None" = None,
    ) -> dict[str, STRiskVerdict]:
        """综合筛选：财务 + 行情（任一 hard → 剔除；warn → 保留但标记）。"""
        verdicts: dict[str, STRiskVerdict] = {}
        for sym in symbols:
            reasons: list[str] = []
            severity = "clean"

            mets = (fin_metrics or {}).get(sym)
            if mets is not None:
                v = self.judge_financials(sym, mets)
                reasons.extend(v.reasons)
                if v.severity == "hard":
                    severity = "hard"
                elif v.severity == "warn" and severity == "clean":
                    severity = "warn"

            closes = (catalog_bars or {}).get(sym)
            if closes:
                v = self.judge_market(sym, closes, (catalog_turnovers or {}).get(sym))
                reasons.extend(v.reasons)
                if v.severity == "hard":
                    severity = "hard"
                elif v.severity == "warn" and severity == "clean":
                    severity = "warn"

            if not reasons and mets is None and not closes:
                reasons.append("no-data")
                severity = "hard"

            verdicts[sym] = STRiskVerdict(sym, severity == "hard", tuple(reasons), severity)
        return verdicts

    @staticmethod
    def apply_pool(verdicts: dict[str, STRiskVerdict], *, drop_warn: bool = False) -> list[str]:
        """从判定结果生成最终股票池。"""
        out = []
        for sym, v in verdicts.items():
            if v.excluded:
                continue
            if drop_warn and v.severity == "warn":
                continue
            out.append(sym)
        return out
