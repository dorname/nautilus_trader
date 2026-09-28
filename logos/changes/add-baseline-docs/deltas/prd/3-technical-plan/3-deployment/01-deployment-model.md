# 部署模型（Deployment Model）

> 状态：基线 · 真相源：docs/getting_started/installation.md、Makefile、crates/cli、pyproject.toml
> 本项目为引擎/库，OpenLogos"部署"定义为：**用户环境内安装并运行一个交易节点**

## 1. 部署形态

| 形态 | 安装方式 | 运行方式 |
|---|---|---|
| Python 用户（主路径） | `pip/uv install nautilus_trader`（PyPI wheel，预编译） | Python 进程内 `BacktestNode`/`LiveExecNode` |
| Rust 用户 | cargo 依赖（crates.io: nautilus-core 等）或源码构建 | Rust 二进制/嵌入式进程 |
| 源码开发 | `make build`（Makefile 全流程） | 开发/测试/文档构建 |

## 2. 环境要求

- 平台：Linux x86_64/ARM64、macOS ARM64、Windows x86_64（NFR-006）
- Rust 1.98.1（rust-toolchain.toml）/ Python 3.12–3.14
- 凭证：venue API keys 经环境变量/.env 注入（.env.example 模板；NFR-009 凭证不入库）

## 3. 部署场景与验收

### 场景 A：研究回测（无外部依赖）
安装 wheel → 加载历史数据 catalog → 运行 BacktestNode → 产出绩效报告。
无需任何 venue 凭证；可在本机/CI 完全离线运行。

### 场景 B：实盘节点（生产）
安装 → 配置 TradingNodeConfig + venue 凭证 → 启动 LiveExecNode →
websocket 连接 venue → 订单链路验证（小额/测试单）→ 转正式运行。
生产建议：进程守护（systemd/k8s）+ 外部监控（日志/事件导出）+ 事件溯源开启（崩溃恢复 FR-005）。

## 4. OpenLogos 部署阶段映射

- 本提案（文档基线）**无需部署**：纯文档变更，不触碰构建产物
- 项目级 `deployment_gates.core`（staging + smoke required）适用于**未来代码类变更提案**：
  verify PASS → 人类确认 → 部署 staging → 冒烟（`openlogos smoke --env staging`）→ 归档
- 冒烟用例规格：见 `02-smoke-test-spec.md`

## 5. 回滚

- 库形态回滚 = 版本回退（pip install nautilus_trader==<旧版> / cargo 依赖锁定）
- 节点进程回滚 = 停进程 → 恢复事件溯源重放点 → 以旧版本重启
