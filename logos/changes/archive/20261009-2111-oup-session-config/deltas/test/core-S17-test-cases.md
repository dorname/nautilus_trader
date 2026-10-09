## ADDED — OUP 会话配置与项目隔离用例（oup-session-config 提案，issue #27/#24/#14/#20）

# OUP 会话配置与项目隔离用例（oup-session-config 提案，crates/research-desktop tests，reporter 入账）

会话配置口径：session_id 必须绑定桌面项目 ID（`research-{project_id}`），
切换/新建项目后旧 worker 必须销毁重建；profile 从 base 数据目录解析
（禁止机器绝对路径硬编码），缺失时早失败；子进程以 workspace_root 为
cwd、stderr 落盘可排障、profile env_vars 显式注入。

| ID | 验收点 | 输入与步骤 | 必须断言 |
|---|---|---|---|
| UT-S17-23 | S17-AC-01 | 无 LLM：两项目分别构造 AgentSessionConfig::desktop；GUI 安装 worker 后切换项目 | 两项目 session_id 与数据目录互不相同；切换/新建项目后 worker 被重置（下次任务绑定新 session）；在途任务引用被放弃并记录 |
| UT-S17-24 | S17-AC-01 | 无 LLM：AgentBridge::connect（spawn octos，不发起 LLM 调用） | 子进程 cwd 为 workspace_root；`{data_dir}/logs/octos-stderr.log` 落盘存在；profile env_vars 注入子进程环境（进程可见，不泄漏到日志） |

### reporter
纯本地配置/进程断言（octos serve 仅启动不调用 LLM，无 API 费用）；
结果经共享 reporter 写入 test-results.jsonl，fail 含 error，skip 不可计为通过。
