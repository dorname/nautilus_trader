# 冒烟测试规格（Smoke Test Spec）

> 状态：基线 · 对接 OpenLogos `openlogos smoke --env staging`
> 部署后冒烟：验证"安装产物在目标环境可用"，与单元/场景测试（代码正确性）互补

## 环境

- staging：隔离的冒烟环境（本机容器/CI runner），使用 sandbox 适配器或录制数据，不接真实资金场所

## 冒烟用例

### SMK-01 安装冒烟（Python 主路径）
**步骤**：目标环境安装 wheel → `python -c "import nautilus_trader; print(nautilus_trader.__version__)"`
**通过标准**：import 成功，版本号与预期发布线一致（v2.x）
**验证点**：NFR-006 平台可用性、依赖完整性（动态库/PyO3 扩展加载）

### SMK-02 回测冒烟
**步骤**：运行最小回测脚本（内置示例数据 + 総略 EMA cross，参考 docs/getting_started/quickstart）
**通过标准**：BacktestNode 运行完成，产出至少 1 笔订单事件与绩效报告，无异常退出
**验证点**：FR-002、NFR-003（同环境重复运行结果一致）

### SMK-03 sandbox 实盘链路冒烟
**步骤**：LiveExecNode + sandbox 适配器启动 → 订阅行情 → 提交测试单 → 收到执行事件 → 优雅停止
**通过标准**：订单事件链完整（Submitted→Accepted→Filled），进程正常退出
**验证点**：FR-010、FR-013、FR-003

### SMK-04 持久化与恢复冒烟
**步骤**：带事件记录运行 SMK-03 → kill -9 模拟崩溃 → 重启并恢复
**通过标准**：恢复后 Cache 中订单/仓位状态与崩溃前一致
**验证点**：FR-004、FR-005、NFR-004

### SMK-05 凭证安全冒烟
**步骤**：以环境变量注入假凭证运行 → 检查日志与产物
**通过标准**：任何日志/报告中不出现明文凭证
**验证点**：NFR-009

## 执行与记录

- 结果写入 `logos/resources/verify/smoke-results.jsonl`（logos.config.json smoke.result_path）
- 报告生成 `logos/resources/verify/smoke-report.md`
- OpenLogos 流程：`[deploy]` 任务完成且 verify PASS 后，人类确认执行 `openlogos smoke --env staging`
