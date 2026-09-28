# 非功能性需求（Non-Functional Requirements）

> 状态：基线 · 量化指标来源：仓库 CI/构建配置与文档承诺；无公开基准数值的项以"可测验收方式"表述

## 性能

### NFR-001 数据面延迟（P0）
Rust 内核消息路径（事件入总线→策略回调→订单命令出）不引入解释器开销；Python 桥接仅存在于控制面与策略接口。
验收方式：Python-free 路径微基准（crates 内 unit/ benches）无回归；CI 含 codspeed 性能看护。

### NFR-002 吞吐（P1）
引擎支持高频事件流（行情 tick 级）持续处理不丢事件。
验收方式：回测高密度数据集（tardis L2）端到端跑通且事件计数一致。

## 可靠性

### NFR-003 回测确定性（P0）
同一输入（数据集+配置+种子）产出逐字节一致的执行事件流与绩效结果。
验收方式：CI 确定性回测用例比对两次运行结果哈希一致。

### NFR-004 崩溃一致性（P0）
任意时刻崩溃，重启重放后状态与崩溃前一致（crash-only，无特殊停机路径）。
验收方式：testkit 故障注入重放测试。

### NFR-005 可观测性（P1）
结构化日志、事件流可导出、消息总线可查询；支持外部监控接入。
验收方式：docs/concepts/logging.md 描述的日志器配置生效；事件可序列化导出。

## 兼容性与约束

### NFR-006 平台支持（P0）
Linux x86_64/ARM64、macOS ARM64、Windows x86_64 全平台构建与测试。
验收方式：CI 构建矩阵（README 平台表：Rust 1.98.1 / Python 3.12-3.14）。

### NFR-007 语言版本（P0）
Rust edition 2024（rustc ≥ 1.98.1），Python 3.12–3.14。
验收方式：rust-toolchain.toml、CI 矩阵。

### NFR-008 许可证（P0）
LGPL-3.0-only，允许商业集成但衍生修改须开源。
验收方式：LICENSE、Cargo.toml workspace.package.license。

## 安全

### NFR-009 凭证安全（P0）
交易所凭证不入库不入日志；支持环境变量/配置注入（.env.example、.gitleaks.toml 扫描）。
验收方式：gitleaks CI 通过；日志脱敏用例。

### NFR-010 供应链安全（P1）
依赖经 cargo-deny（deny.toml）、osv-scanner、security-audit 审计；补丁经 patches/ 目录管理。
验收方式：CI 供应链作业全绿。

## 工程质量

### NFR-011 测试覆盖与静态检查（P0）
全 workspace `cargo test` + clippy + rustfmt + codespell + markdownlint + lychee 链接检查通过。
验收方式：pre-commit（.pre-commit-config.yaml）与 CI。

### NFR-012 文档完备（P1）
概念/指南/教程/集成/参考五类文档（docs/）与 Rust API reference 全量发布。
验收方式：docs 站点构建（docs/api_reference）。
