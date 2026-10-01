"""astock 测试的 OpenLogos reporter（批次 7）。

从测试函数名解析用例 ID（test_ut_ast_01_x → UT-AST-01；test_st_a1_x → ST-A1；
复合覆盖如 test_ut_ast_05_06_x 展开为两条），按 OpenLogos reporter 字段
（id/status/duration_ms/timestamp/error）追加写入 logos/resources/verify/test-results.jsonl。

约定：本 hook 只追加、不清空——清理由 verify 的 pre_run_command 负责；
独立运行 pytest 前请自行 rm 该文件。skip 不可计为通过。
"""

from __future__ import annotations

import json
import re
from datetime import datetime, timezone
from pathlib import Path

import pytest

_ROOT = Path(__file__).resolve().parents[4]
_RESULTS = _ROOT / "logos" / "resources" / "verify" / "test-results.jsonl"

# test_<前缀>_<域><序号>(_序号)*_：域与序号间下划线可选——UT-AST-01 连字符输出，ST-A1 直连输出
_CASE_RE = re.compile(r"test_((ut|st)_([a-z]+?)_?(\d+(?:_\d+)*))_", re.IGNORECASE)
_ERROR_MAX = 500
_DURATION_KEY = "_openlogos_duration_s"


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


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item, call):  # noqa: ANN001, ANN201
    outcome = yield
    report = outcome.get_result()
    # 累计 setup/call/teardown 三个阶段时长（挂在 item 上），teardown 时落盘
    total = getattr(item, _DURATION_KEY, 0.0) + report.duration
    setattr(item, _DURATION_KEY, total)
    if report.when != "teardown":
        return
    ids = _case_ids(item.name)
    if not ids:
        return
    if report.passed:
        status, error = "pass", None
    elif report.failed:
        status = "fail"
        error = str(report.longrepr)[:_ERROR_MAX] if report.longrepr else "assertion failed"
    else:
        status, error = "skip", str(report.longrepr)[:_ERROR_MAX] if report.longrepr else "skipped"
    record_base = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "duration_ms": round(total * 1000),
    }
    _RESULTS.parent.mkdir(parents=True, exist_ok=True)
    with _RESULTS.open("a", encoding="utf-8") as fh:
        for case_id in ids:
            record = {"id": case_id, "status": status, **record_base}
            if error is not None:
                record["error"] = error
            fh.write(json.dumps(record, ensure_ascii=False) + "\n")
