## ADDED — 路径绝对化回归用例（fix-issues-38-39-20261013 / issue #38）

| 用例 ID | 验收条件 | 场景 | 预期 |
|---------|---------|------|------|
| UT-S17-32 | S17-AC-01 | 无 LLM：`AgentSessionConfig::desktop_with_profile` 以**相对路径** base_dir 构造 | 产出的 `data_dir` / `instance_dir` / `workspace_root` 均为绝对路径（canonicalize）；profile 复制目标与子进程 `--data-dir` 视图一致（父进程写入路径 == 配置中的 data_dir/profiles）；临时目录隔离清理 |
