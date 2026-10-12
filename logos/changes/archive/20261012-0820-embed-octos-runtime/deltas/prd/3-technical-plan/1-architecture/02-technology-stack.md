# Delta: 02-technology-stack.md

## MODIFIED — 2. Rust 依赖策略（关键项）

- 异步运行时：tokio 生态（live/network 适配器并发）
- 序列化：msgpack/serde 系（serialization crate）
- 数据存储：parquet（catalog，persistence crate）
- 依赖治理：cargo-deny（deny.toml 许可/禁投依赖）、osv-scanner、audit（NFR-010）
- 补丁管理：仓库 patches/ 目录管理第三方补丁
- **OUP 协议类型（embed-octos-runtime）**：`octos-core` 以 git 依赖精确 pin（`rev` 与桌面内嵌的 octos release 一一对应，参考 octoscode 同款做法），提供 UI 协议请求/响应/通知的类型化定义与能力协商常量；版本 pin 漂移即编译期失败，防协议静默不匹配
- **内嵌 octos 运行时（embed-octos-runtime）**：research-desktop 分发物随包 octos 二进制（bundle），构建/打包清单登记版本与许可证；运行时按「内嵌资源 → `~/.octos/bin` → PATH」解析（详见 core-06 第 7 节），不依赖用户 PATH
