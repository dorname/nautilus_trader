"""A股数据接入（a-stock 分支）。

双源日线数据加载层：
- ``tdx_loader``：通达信（TDX）人工导出的 ``.day`` 静态日线文件解析
- ``tickflow_client``：TickFlow API（https://docs.tickflow.org/zh-Hans）在线日线
- ``catalog_writer``：双源合并写入 Nautilus Parquet catalog

规格依据：logos/resources/prd/1-product-requirements/07-astock-requirements.md
架构依据：logos/resources/prd/3-technical-plan/1-architecture/04-astock-data-architecture.md
"""

from __future__ import annotations

from typing import Any

from nautilus_trader.adapters.astock.catalog_writer import AstockCatalogWriter
from nautilus_trader.adapters.astock.catalog_writer import AstockMergedBar
from nautilus_trader.adapters.astock.tdx_loader import TdxDayRecord
from nautilus_trader.adapters.astock.tdx_loader import parse_tdx_day_file
from nautilus_trader.adapters.astock.tdx_loader import scan_tdx_dir
from nautilus_trader.adapters.astock.tickflow_client import TickFlowClient
from nautilus_trader.adapters.astock.tickflow_client import TickFlowKline

__all__ = (
    "AstockCatalogWriter",
    "AstockMergedBar",
    "TickFlowClient",
    "TickFlowKline",
    "TdxDayRecord",
    "parse_tdx_day_file",
    "scan_tdx_dir",
)


def build_astock_catalog(
    tdx_dir: str | None,
    catalog_path: str,
    symbols_tickflow: list[str] | None = None,
    tickflow_api_key: str | None = None,
    tickflow_count: int = 10000,
    tickflow_adjust: str = "none",
    **writer_kwargs: Any,
) -> dict[str, Any]:
    """双源共建 A股日线 catalog（规格 FR-A1，时序图 D-11）。

    Parameters
    ----------
    tdx_dir : str | None
        通达信导出根目录（如 ``vipdoc``）。None 则跳过静态源。
    catalog_path : str
        Nautilus Parquet catalog 路径。
    symbols_tickflow : list[str] | None
        经 TickFlow 增量拉取的标的（如 ``["600000.SH"]``）。空/None 跳过在线源。
    tickflow_api_key : str | None
        API Key；缺省读环境变量 ``TICKFLOW_API_KEY``（NFR-A1）。
    tickflow_adjust : str
        TickFlow 复权类型 ``none|qfq|hfq``（FR-A6）。

    Returns
    -------
    dict[str, Any]
        建库报告：各源贡献条数、冲突覆盖数、跳过文件数。

    """
    writer = AstockCatalogWriter(catalog_path)
    report: dict[str, Any] = {
        "catalog": catalog_path,
        "tdx_bars": 0,
        "tdx_files": 0,
        "tdx_skipped": [],
        "tickflow_bars": 0,
        "tickflow_errors": [],
        "conflicts_resolved": 0,
        "total_written": 0,
    }

    # ---- 静态源：通达信 .day（US-A1）----
    if tdx_dir:
        for path, records_or_err in scan_tdx_dir(tdx_dir):
            if isinstance(records_or_err, str):
                report["tdx_skipped"].append(records_or_err)
                continue
            report["tdx_files"] += 1
            writer.add_tdx(records_or_err)
            report["tdx_bars"] += len(records_or_err)

    # ---- 在线源：TickFlow 增量（US-A2）----
    if symbols_tickflow:
        client = TickFlowClient(api_key=tickflow_api_key)
        for symbol in symbols_tickflow:
            try:
                klines = client.fetch_daily(
                    symbol=symbol,
                    count=tickflow_count,
                    adjust=tickflow_adjust,
                )
            except Exception as exc:  # noqa: BLE001 — 单标的失败不阻塞其余（NFR-A2）
                report["tickflow_errors"].append(f"{symbol}: {exc}")
                continue
            report["tickflow_bars"] += len(klines)
            writer.add_tickflow(symbol, klines)

    # ---- 合并落盘（FR-A4：ts_event 去重，TickFlow 优先）----
    merged, conflicts = writer.merge_and_write(**writer_kwargs)
    report["conflicts_resolved"] = conflicts
    report["total_written"] = len(merged)
    return report
