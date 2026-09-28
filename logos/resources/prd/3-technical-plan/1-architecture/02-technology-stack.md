# 技术选型（Technology Stack）

> 状态：基线 · 真相源：Cargo.toml（workspace 0.65.0, edition 2024）、rust-toolchain.toml、python/pyproject.toml、Makefile、CI

## 1. 语言与工具链

| 项 | 选择 | 版本 | 依据 |
|---|---|---|---|
| 数据面语言 | Rust | rustc 1.98.1（rust-toolchain.toml 锁定）、edition 2024 | 性能 + 内存安全 + 无 GC |
| 控制面语言 | Python | 3.12–3.14（CI 矩阵） | 研究生态、策略迭代速度 |
| 语言桥接 | PyO3（crates/pyo3） | workspace 内维护 | Rust↔Python FFI，控制面调用内核 |

**选型理由**：交易引擎的延迟敏感路径（事件分发、订单状态机）留在 Rust；策略/配置/编排的
迭代效率由 Python 承担。纯 Rust 路径也完整保留（FR-007，US-010）。

## 2. Rust 依赖策略（关键项）

- 异步运行时：tokio 生态（live/network 适配器并发）
- 序列化：msgpack/serde 系（serialization crate）
- 数据存储：parquet（catalog，persistence crate）
- 依赖治理：cargo-deny（deny.toml 许可/禁投依赖）、osv-scanner、audit（NFR-010）
- 补丁管理：仓库 patches/ 目录管理第三方补丁

## 3. Python 侧

- 包结构 `python/nautilus_trader` 完整镜像 Rust 域结构（model/data/execution/…）
- stubs 生成：generate_stubs.py（.pyi），docstring 生成：generate_docstrings.py
- 打包：pyproject.toml（wheel 分发，pypi: nautilus_trader）

## 4. 测试与质量工具链

| 工具 | 用途 |
|---|---|
| cargo test | 全 workspace 单元/集成测试（logos verify 预跑命令） |
| codspeed | 性能回归看护 |
| clippy / rustfmt | 静态检查与格式 |
| pre-commit（19 hooks） | codespell、markdownlint、lychee、yaml/toml 校验等 |
| gitleaks | 凭证泄漏扫描（NFR-009） |

## 5. 构建与发布

- 版本线：master / nightly / develop 三分支（version.json 当前 v2.0.0rc6）
- Makefile（~71KB）：构建/测试/文档/发布任务统一入口
- 平台矩阵：Linux x86_64/ARM64、macOS ARM64、Windows x86_64（NFR-006）
- 许可：LGPL-3.0-only（NFR-008）

## 6. 选型权衡记录（如实）

- **PyO3 桥接成本**：跨语言调用有开销 → 缓解：高频对象 Rust 内闭环，桥接仅控制面
- **LGPL 而非 MIT/Apache**：商业动态链接友好但静态修改须开源 → 与开源策略一致
- **无自有数据库**：状态 backing 外置（Redis/parquet 可选） → 换取部署灵活性
