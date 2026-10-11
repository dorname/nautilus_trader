# Delta: core-03-desktop-delivery.md

## MODIFIED — 构建与运行依赖

在 Windows MSVC 与 Ubuntu 本机 CI 分别构建 Rust GUI／worker；遵循仓库 Rust 1.98.1 与锁文件，依赖锁定后记录构建SHA和工具链。初次实现必须先验证 GUI、引擎与Python桥接可同时打包；失败时修正实现方案，不交付只能运行截图的壳。
Python 桥接使用随包隔离运行时，打包其实际所需的Parquet依赖和适配器，不依赖用户PATH中的Python。运行时、字体和动态库均写入版本／许可证清单；离线启动不得临时下载依赖。中文字体随包，选择允许再分发的字体并携带许可证。图形初始化失败给出中文诊断与日志入口，不能假定服务器无显示环境可通过GUI验收。
图形渲染使用eframe的wgpu路径；实现首批验证目标机驱动。没有可用图形后端时启动失败并提示，不把软件渲染支持当作已验证承诺。

**octos 运行时随包（embed-octos-runtime）**：Agent 内核 `octos` 二进制以 bundle 形式随包分发（对齐 Python 桥接「随包隔离运行时、不依赖用户 PATH」先例），打包版本与客户端 `octos-core` 协议 pin（`REQUIRED_OCTOS_RELEASE`）一一对应并写入版本／许可证清单。运行时解析顺序：内嵌资源 → `~/.octos/bin` → PATH（详见 core-06 第 7 节）；三级全缺失时给出中文引导卡片与安装指引，离线启动不自动下载。版本不匹配（非内嵌来源命中且 `--version` 校验失败）同样显式诊断，不静默降级运行。
