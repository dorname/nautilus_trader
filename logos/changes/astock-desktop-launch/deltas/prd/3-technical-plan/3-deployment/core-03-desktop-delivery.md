## ADDED — 本地开发启动前置

## 本地开发启动前置

本提案（astock-desktop-launch）交付本地启动形态，不执行部署、不宣称安装包已存在。构建前置：

- **egui/eframe/egui_plot 依赖获取**：网络可用时 `cargo fetch` 后锁定 Cargo.lock；
  离线环境以 `cargo vendor` 离线化或写入本地缓存。实际获取方式与锁定版本记录于
  实现 manifest。锁定前不进入后续批次。
- **本地启动入口**：批次 L1 后 `cargo run -p nautilus-research-cli -- --workspace <路径> <子命令>`；
  批次 L2 后 `cargo run -p nautilus-research-desktop`。
- **无显示环境（WSL2 无 WSLg／服务器）**：GUI crate 保持可编译，验收以 CLI 链路＋
  headless 单元测试承载；渲染与缩放类用例诚实标注 [manual]／skip，不以软件渲染
  截图冒充双平台 GUI 验收（与上文「不把软件渲染支持当作已验证承诺」一致）。
- **双平台安装包交付与 smoke**：维持上文门禁——独立部署级提案＋用户授权后才执行；
  本提案完成不改变该边界。
