"""astock 测试的 OpenLogos reporter（批次 astock-st-risk-filter）。

从测试函数名解析用例 ID（test_ut_stf_01_x → UT-STF-01；test_st_stf_01_x → ST-STF-01），
按 OpenLogos reporter 字段（id/status/duration_ms/timestamp/error）追加写入 JSONL。

本次修复：真实聚合 setup / call / teardown 三阶段状态，优先级 fail > skip > pass；
call/setup 失败不会被成功 teardown 掩盖；teardown 失败最终为 fail；duration 累计三阶段。

约定：默认输出路径 `logos/resources/verify/test-results.jsonl`，可通过环境变量
`OPENLOGOS_RESULTS_PATH` 覆盖（仅用于自测），默认行为不变。
"""

from __future__ import annotations

import json
import os
import re
from datetime import datetime, timezone
from pathlib import Path

import pytest

_ROOT = Path(__file__).resolve().parents[4]
_RESULTS = Path(os.environ.get("OPENLOGOS_RESULTS_PATH", _ROOT / "logos" / "resources" / "verify" / "test-results.jsonl"))

# test_<前缀>_<域><序号>(_序号)*_：域与序号间下划线可选——UT-STF-01 连字符输出，ST-STF-01 直连输出
_CASE_RE = re.compile(r"test_((ut|st)_([a-z]+?)_?(\d+(?:_\d+)*))_", re.IGNORECASE)
_ERROR_MAX = 500
_DURATION_KEY = "_openlogos_duration_s"
_STATUS_KEY = "_openlogos_status"
_ERROR_KEY = "_openlogos_error"


def _case_ids(node_name: str) -> list[str]:
    match = _CASE_RE.match(node_name)
    if not match:
        return []
    _full, prefix, domain, nums = match.groups()
    ids: list[str] = []
    for num in nums.lstrip("_").split("_"):
        if len(domain) == 1:
            ids.append(f"{prefix.upper()}-{domain.upper()}{num}")
        else:
            ids.append(f"{prefix.upper()}-{domain.upper()}-{num}")
    return ids


def _higher_priority(current: str | None, new: str) -> bool:
    """状态优先级：fail > skip > pass。"""
    order = {"pass": 0, "skip": 1, "fail": 2}
    return current is None or order.get(new, 0) > order.get(current, 0)


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item, call):  # noqa: ANN001, ANN201
    outcome = yield
    report = outcome.get_result()

    # 累计三阶段时长（挂在 item 上）
    total = getattr(item, _DURATION_KEY, 0.0) + report.duration
    setattr(item, _DURATION_KEY, total)

    # 按优先级更新跨阶段状态与错误信息
    status: str | None = None
    error: str | None = None
    if report.passed:
        status = "pass"
    elif report.failed:
        status = "fail"
        error = str(report.longrepr)[:_ERROR_MAX] if report.longrepr else "assertion failed"
    elif report.skipped:
        # 规格要求 skip 状态不携带 error，确保 OpenLogos verify 判定一致
        status = "skip"

    if status is not None:
        current_status = getattr(item, _STATUS_KEY, None)
        if _higher_priority(current_status, status):
            setattr(item, _STATUS_KEY, status)
            if error is not None:
                setattr(item, _ERROR_KEY, error)

    # teardown 阶段落盘，确保三阶段已聚合完毕
    if report.when != "teardown":
        return

    ids = _case_ids(item.name)
    if not ids:
        return

    final_status = getattr(item, _STATUS_KEY, "pass")
    final_error = getattr(item, _ERROR_KEY, None)
    record_base = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "duration_ms": round(total * 1000),
    }
    _RESULTS.parent.mkdir(parents=True, exist_ok=True)
    with _RESULTS.open("a", encoding="utf-8") as fh:
        for case_id in ids:
            record = {"id": case_id, "status": final_status, **record_base}
            if final_error is not None:
                record["error"] = final_error
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")
