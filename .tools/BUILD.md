# 构建说明（a-stock 分支）

## 内核构建（已完成，产物 175MB）
target/release/libnautilus_pyo3.so → python/nautilus_trader/_libnautilus.so

## 重建命令（沙箱内每次 shell 独立，后台进程会被杀，必须前台跑）
```
export RUSTUP_HOME=$PWD/.tools/rustup CARGO_HOME=$PWD/.tools/cargo-home
export PATH="$PWD/.tools/cargo-home/bin:$PWD/.tools/bin:$PATH"
export PYO3_PYTHON=$PWD/.tools/pythons/cpython-3.12.14-linux-x86_64-gnu/bin/python3
nice -n 19 taskset -c 0-7 cargo build --release -p nautilus-pyo3 \
  --features "extension-module,arrow,high-precision,mimalloc,redis,postgres,tracing-bridge"
cp target/release/libnautilus_pyo3.so python/nautilus_trader/_libnautilus.so
```

## 运行 Python（系统 3.10 太老：PyBaseExceptionGroup 需 3.11+）
```
PY=.tools/pythons/cpython-3.12.14-linux-x86_64-gnu/bin/python3
PYTHONPATH=python $PY ...
```
依赖已装到该解释器：pyarrow pandas

## CPU 约束
所有重活：nice -n 19 + taskset -c 0-7（8核/最低优先级）
