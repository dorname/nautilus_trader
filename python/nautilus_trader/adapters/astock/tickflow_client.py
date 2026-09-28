"""TickFlow 行情数据客户端（在线日线源）。

规格：FR-A3（GET /v1/klines）、FR-A6（adjust 透传）、FR-A8（增量 start_time）、
NFR-A1（API Key 环境变量）、NFR-A2（限速退避，单源失败不阻塞）。
契约：logos/resources/api/tickflow-klines.yaml（官方文档 2026-09-28 抓取）。
"""

from __future__ import annotations

import os
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass

DEFAULT_BASE_URL = "https://api.tickflow.org"
KLINE_PATH = "/v1/klines"
PERIOD_DAILY = "1d"
MAX_COUNT = 10000  # 官方约束
_RETRY_STATUS = {429, 500, 502, 503, 504}
_MAX_RETRIES = 5
_API_KEY_ENV = "TICKFLOW_API_KEY"  # NFR-A1


class TickFlowAuthError(RuntimeError):
    """401/403：API Key 缺失或无效。"""


class TickFlowNotFoundError(RuntimeError):
    """404：标的不存在。"""


@dataclass(frozen=True, slots=True)
class TickFlowKline:
    """一条 TickFlow K线（元/股/毫秒时间戳，与 API 原样对应）。"""

    symbol: str  # 如 "600000.SH"
    timestamp_ms: int
    open: float
    high: float
    low: float
    close: float
    volume: int
    amount: float | None = None
    adjust: str = "none"


class TickFlowClient:
    """TickFlow K线 REST 客户端（stdlib 实现，无第三方依赖）。"""

    def __init__(
        self,
        api_key: str | None = None,
        base_url: str = DEFAULT_BASE_URL,
        timeout: float = 15.0,
        max_retries: int = _MAX_RETRIES,
    ) -> None:
        self._api_key = api_key if api_key is not None else os.environ.get(_API_KEY_ENV, "")
        self._base_url = base_url.rstrip("/")
        self._timeout = timeout
        self._max_retries = max(0, int(max_retries))

    @property
    def has_api_key(self) -> bool:
        return bool(self._api_key)

    def fetch_daily(
        self,
        symbol: str,
        *,
        count: int = 100,
        start_time_ms: int | None = None,
        end_time_ms: int | None = None,
        adjust: str = "none",
    ) -> list[TickFlowKline]:
        """查询日线（period=1d）。参数语义见 tickflow-klines.yaml。"""
        return self.fetch_klines(
            symbol,
            period=PERIOD_DAILY,
            count=count,
            start_time_ms=start_time_ms,
            end_time_ms=end_time_ms,
            adjust=adjust,
        )

    def fetch_klines(
        self,
        symbol: str,
        *,
        period: str = PERIOD_DAILY,
        count: int = 100,
        start_time_ms: int | None = None,
        end_time_ms: int | None = None,
        adjust: str = "none",
    ) -> list[TickFlowKline]:
        if not self._api_key:
            raise TickFlowAuthError(
                f"missing API key: pass api_key or set {_API_KEY_ENV}",
            )
        if not 0 <= count <= MAX_COUNT:
            raise ValueError(f"count must be in [0, {MAX_COUNT}], got {count}")

        query: dict[str, str] = {"symbol": symbol, "period": period, "adjust": adjust}
        if count:
            query["count"] = str(count)
        if start_time_ms is not None:
            query["start_time"] = str(int(start_time_ms))
        if end_time_ms is not None:
            query["end_time"] = str(int(end_time_ms))

        url = f"{self._base_url}{KLINE_PATH}?{urllib.parse.urlencode(query)}"
        payload = self._request_json(url)
        return self._parse_response(payload, symbol=symbol, adjust=adjust)

    # ------------------------------------------------------------------

    def _request_json(self, url: str) -> dict:
        """带指数退避的 GET（NFR-A2）；401/403/404 直接抛专属异常。"""
        last_error: Exception | None = None
        for attempt in range(self._max_retries + 1):
            request = urllib.request.Request(  # noqa: S310 — 固定 https base
                url,
                headers={"x-api-key": self._api_key, "Accept": "application/json"},
                method="GET",
            )
            try:
                with urllib.request.urlopen(request, timeout=self._timeout) as response:  # noqa: S310
                    import json

                    return json.loads(response.read().decode("utf-8"))
            except urllib.error.HTTPError as exc:
                if exc.code in (401, 403):
                    raise TickFlowAuthError(f"HTTP {exc.code}: check {_API_KEY_ENV}") from exc
                if exc.code == 404:
                    raise TickFlowNotFoundError(f"symbol not found: {url}") from exc
                last_error = exc
                if exc.code not in _RETRY_STATUS:
                    raise RuntimeError(f"HTTP {exc.code}") from exc
            except (urllib.error.URLError, TimeoutError, OSError) as exc:
                last_error = exc

            if attempt < self._max_retries:
                time.sleep(min(2.0**attempt, 30.0))  # 指数退避，封顶 30s
        raise RuntimeError(f"tickflow request failed after retries: {last_error}")

    @staticmethod
    def _parse_response(
        payload: dict,
        *,
        symbol: str,
        adjust: str,
    ) -> list[TickFlowKline]:
        """解析 KlinesResponse（UT-AST-03）。容忍 items/data 两种字段名。"""
        raw_items = payload.get("items") or payload.get("data") or []
        klines: list[TickFlowKline] = []
        for item in raw_items:
            ts = item.get("timestamp") or item.get("time") or item.get("ts")
            if ts is None:
                continue
            ts_ms = int(ts)
            # 官方毫秒；若秒级时间戳（<1e12）则归一到毫秒
            if ts_ms < 10**12:
                ts_ms *= 1000
            klines.append(
                TickFlowKline(
                    symbol=symbol,
                    timestamp_ms=ts_ms,
                    open=float(item["open"]),
                    high=float(item["high"]),
                    low=float(item["low"]),
                    close=float(item["close"]),
                    volume=int(item.get("volume") or 0),
                    amount=float(item["amount"]) if item.get("amount") is not None else None,
                    adjust=adjust,
                ),
            )
        return klines
