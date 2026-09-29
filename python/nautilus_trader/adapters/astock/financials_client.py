"""TickFlow 财务数据客户端 — ST 风险筛选数据源。

契约：GET /v1/financials/metrics（官方文档 2026-09-29 抓取）
  - symbols: 逗号分隔，如 600519.SH,000001.SZ（批量）
  - start_date/end_date: period_end 过滤（YYYY-MM-DD）
  - latest: 仅最新一期（默认 false）
认证：x-api-key；与 tickflow_client 同源复用退避/错误处理。
"""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

from nautilus_trader.adapters.astock.tickflow_client import DEFAULT_BASE_URL
from nautilus_trader.adapters.astock.tickflow_client import TickFlowAuthError

METRICS_PATH = "/v1/financials/metrics"
_MAX_RETRIES = 5


@dataclass(frozen=True, slots=True)
class FinancialMetrics:
    """一期核心财务指标（字段以 API 实际返回为准，宽松解析）。"""

    symbol: str
    period_end: str  # 如 "2026-06-30"
    roe: float | None = None  # 净资产收益率（%）
    eps: float | None = None  # 每股收益（元）
    bps: float | None = None  # 每股净资产（元）—— ST 核心判据
    net_profit: float | None = None  # 净利润（元）—— 连亏判据
    revenue_growth: float | None = None  # 营收增速（%）
    debt_ratio: float | None = None  # 资产负债率（%）
    raw: dict | None = None

    @property
    def is_loss(self) -> bool:
        """当期净利润为负。"""
        return self.net_profit is not None and self.net_profit < 0

    @property
    def negative_equity(self) -> bool:
        """每股净资产为负（资不抵债）→ 直接 ST 风险。"""
        return self.bps is not None and self.bps < 0


class TickFlowFinancialsClient:
    """财务指标批量查询（复用 K线客户端的认证与退避）。"""

    def __init__(self, api_key: str | None = None, base_url: str = DEFAULT_BASE_URL, timeout: float = 20.0) -> None:
        from nautilus_trader.adapters.astock.tickflow_client import TickFlowClient

        self._inner = TickFlowClient(api_key=api_key, base_url=base_url, timeout=timeout)

    def fetch_metrics(
        self,
        symbols: list[str],
        *,
        start_date: str | None = None,
        end_date: str | None = None,
        latest: bool = False,
    ) -> list[FinancialMetrics]:
        if not self._inner.has_api_key:
            raise TickFlowAuthError("missing TICKFLOW_API_KEY for financials")
        query: dict[str, str] = {"symbols": ",".join(symbols)}
        if start_date:
            query["start_date"] = start_date
        if end_date:
            query["end_date"] = end_date
        if latest:
            query["latest"] = "true"
        url = f"{self._inner._base_url}{METRICS_PATH}?{urllib.parse.urlencode(query)}"
        payload = self._request_json(url)
        return self._parse(payload)

    def _request_json(self, url: str) -> dict:
        last: Exception | None = None
        for attempt in range(_MAX_RETRIES + 1):
            req = urllib.request.Request(  # noqa: S310
                url,
                headers={"x-api-key": self._inner._api_key, "Accept": "application/json"},
                method="GET",
            )
            try:
                with urllib.request.urlopen(req, timeout=15.0) as resp:  # noqa: S310
                    return json.loads(resp.read().decode("utf-8"))
            except urllib.error.HTTPError as exc:
                if exc.code in (401, 403):
                    raise TickFlowAuthError(f"HTTP {exc.code}") from exc
                last = exc
                if exc.code not in (429, 500, 502, 503, 504):
                    raise RuntimeError(f"HTTP {exc.code}") from exc
            except (urllib.error.URLError, TimeoutError, OSError) as exc:
                last = exc
            if attempt < _MAX_RETRIES:
                time.sleep(min(2.0**attempt, 30.0))
        raise RuntimeError(f"tickflow financials failed: {last}")

    @staticmethod
    def _parse(payload: dict) -> list[FinancialMetrics]:
        items = payload.get("items") or payload.get("data") or []
        out: list[FinancialMetrics] = []
        for it in items:
            out.append(
                FinancialMetrics(
                    symbol=str(it.get("symbol") or it.get("code") or ""),
                    period_end=str(it.get("period_end") or it.get("report_date") or ""),
                    roe=_f(it, ("roe", "ROE", "return_on_equity")),
                    eps=_f(it, ("eps", "EPS", "basic_eps")),
                    bps=_f(it, ("bps", "BPS", "net_asset_per_share", "book_value_per_share")),
                    net_profit=_f(it, ("net_profit", "netProfit", "net_income")),
                    revenue_growth=_f(it, ("revenue_growth", "revenueGrowth", "yoy_revenue")),
                    debt_ratio=_f(it, ("debt_ratio", "debtRatio", "asset_liability_ratio")),
                    raw=it,
                ),
            )
        return out


def _f(d: dict, keys: tuple[str, ...]) -> float | None:
    for k in keys:
        v = d.get(k)
        if v is None:
            continue
        try:
            return float(v)
        except (TypeError, ValueError):
            continue
    return None
