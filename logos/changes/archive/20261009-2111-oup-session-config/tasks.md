# 实现任务

## [delta] 规格变更
- [x] deltas/test/core-S17-test-cases.md：注册 UT-S17-23 / UT-S17-24（会话配置与隔离口径）

## [code] 代码实现

- [x] AgentWorker/SwarmWorker 记录 session_id；GUI 切换/新建项目时后台销毁旧 worker 并惰性重建（#27）
- [x] swarm 派发 profile 复制去硬编码，改读 base data_dir/profiles；缺失明确报错（#24）；5 个测试文件 workspace_root/profile 路径去机器绝对路径（CARGO_MANIFEST_DIR/HOME 解析）
- [x] OupConfig + extra_env；spawn 传 cwd + stderr 落盘 logs/octos-stderr.log；connect 注入 profile env_vars（#14）
- [x] oup.rs 模块注释补 stdio 能力子集（#20）
- [x] UT-S17-23 / UT-S17-24 入账 reporter 并通过
