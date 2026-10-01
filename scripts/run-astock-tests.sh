#!/bin/sh
# astock Python 测试入口（verify pre_run 第三段）。
# 要求解释器能加载 python/nautilus_trader/_libnautilus.so（当前构建目标 cp312，
# 系统 python3.10 加载报 undefined symbol）。pytest 及其依赖为纯 Python，
# 复用系统 dist-packages；纯路径注入，不安装。
set -e
cd "$(dirname "$0")/.."

PYT="${PYTHON312:-}"
if [ -z "$PYT" ]; then
    for cand in /root/.local/share/uv/python/cpython-3.12*/bin/python3.12 python3; do
        if "$cand" -c "import sys; sys.path.insert(0, 'python'); import nautilus_trader" >/dev/null 2>&1; then
            PYT="$cand"
            break
        fi
    done
fi
if [ -z "$PYT" ]; then
    echo "no python interpreter can load _libnautilus.so (need cp312 build)" >&2
    exit 1
fi

export PYTHONPATH="/usr/local/lib/python3.10/dist-packages:/usr/lib/python3/dist-packages${PYTHONPATH:+:$PYTHONPATH}"
exec "$PYT" -m pytest python/tests/adapters/astock/ -q "$@"
