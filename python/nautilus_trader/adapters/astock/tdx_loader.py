"""通达信（TDX）静态日线文件解析。

规格：FR-A2（.day 二进制解析）、NFR-A3（零第三方依赖、容错）。
格式依据：04-astock-data-architecture.md 的 32 字节记录布局。
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator
from typing import Union

# 每记录 32 字节（小端）：date/open/high/low/close/amount/vol/reserved
_RECORD = struct.Struct("<IIIIIfII")
_RECORD_SIZE = _RECORD.size  # 32

Exchange = str  # "SH" | "SZ"


@dataclass(frozen=True, slots=True)
class TdxDayRecord:
    """一条通达信日线记录（已换算为元/股，日期为 YYYYMMDD int）。"""

    date: int
    open: float
    high: float
    low: float
    close: float
    amount: float  # 成交额（元）
    volume: int  # 成交量（股）

    def validate(self) -> bool:
        """字段合理性校验（UT-AST-01 辅助）。"""
        if not 19900101 <= self.date <= 20991231:
            return False
        if min(self.open, self.close, self.low) <= 0:
            return False
        return self.low <= min(self.open, self.close) and max(self.open, self.close) <= self.high


def _file_to_symbol(path: Path) -> tuple[str, Exchange] | None:
    """从 ``sh600000.day`` / ``sz000001.day`` 提取 (symbol, exchange)。

    支持通达信导出目录的两种常见命名：带市场前缀（vipdoc 布局）与裸代码。
    """
    stem = path.stem.lower()
    if stem.startswith("sh") and stem[2:].isdigit() and len(stem[2:]) == 6:
        return stem[2:], "SH"
    if stem.startswith("sz") and stem[2:].isdigit() and len(stem[2:]) == 6:
        return stem[2:], "SZ"
    if stem.isdigit() and len(stem) == 6:
        # 无前缀时按代码区间推断：6xx→SH，其余→SZ
        return stem, "SH" if stem.startswith(("5", "6", "9")) else "SZ"
    return None


def parse_tdx_day_file(
    path: str | Path,
    *,
    strict: bool = False,
) -> list[tuple[str, Exchange, TdxDayRecord]]:
    """解析单个 ``.day`` 文件。

    Returns
    -------
    list[tuple[str, Exchange, TdxDayRecord]]
        (symbol, exchange, record) 三元组列表，按文件内顺序。

    Raises
    ------
    ValueError
        ``strict=True`` 且文件名无法识别或长度非法时抛出；
        默认（strict=False）返回空列表交由上层告警（NFR-A3 容错）。

    """
    path = Path(path)
    ident = _file_to_symbol(path)
    if ident is None:
        if strict:
            raise ValueError(f"unrecognized tdx file name: {path.name}")
        return []
    symbol, exchange = ident

    data = path.read_bytes()
    if len(data) % _RECORD_SIZE != 0:
        if strict:
            raise ValueError(f"bad tdx file length {len(data)}: {path}")
        return [(symbol, exchange, r) for r in []]  # 显式空

    out: list[tuple[str, Exchange, TdxDayRecord]] = []
    for offset in range(0, len(data), _RECORD_SIZE):
        date, o, h, l, c, amount, vol, _reserved = _RECORD.unpack_from(data, offset)
        rec = TdxDayRecord(
            date=date,
            open=o / 100.0,
            high=h / 100.0,
            low=l / 100.0,
            close=c / 100.0,
            amount=float(amount),
            volume=int(vol),
        )
        if not rec.validate():
            if strict:
                raise ValueError(f"invalid record {date} in {path.name}")
            continue
        out.append((symbol, exchange, rec))
    return out


def scan_tdx_dir(root: str | Path) -> Iterator[tuple[Path, Union[list, str]]]:
    """递归扫描通达信导出目录。

    Yields
    ------
    (path, records) — 解析成功的 ``(symbol, exchange, record)`` 列表
    (path, reason)  — 解析失败的告警字符串（不中断批量，NFR-A3）

    """
    root = Path(root)
    for path in sorted(root.rglob("*.day")):
        try:
            records = parse_tdx_day_file(path)
        except OSError as exc:
            yield path, f"{path}: {exc}"
            continue
        if not records:
            # 无法识别命名 / 全部记录非法 / 长度非法：parse 内部已容错为空
            yield path, f"{path.name}: no valid records (name or format unrecognized)"
        else:
            yield path, records
