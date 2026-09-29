-- 桌面研究元数据 schema v1
-- SQLite 仅由协调器写入；引擎本身不依赖此数据库。
-- 所有 JSON 字段遵循 core-research-contracts.yaml 的契约，写入前验证。
-- 金额在 JSON 中使用十进制字符串；大行情、结果行保存在内容寻址文件中。
PRAGMA foreign_keys = ON;
CREATE TABLE schema_version(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
CREATE TABLE artifact(
 hash TEXT PRIMARY KEY CHECK(length(hash)=64), relative_path TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL, size_bytes INTEGER NOT NULL CHECK(size_bytes>=0), created_at TEXT NOT NULL
);
CREATE TABLE snapshot(
 id TEXT PRIMARY KEY, manifest_hash TEXT NOT NULL REFERENCES artifact(hash),
 as_of TEXT NOT NULL, capabilities_json TEXT NOT NULL CHECK(json_valid(capabilities_json)),
 limitations_json TEXT NOT NULL CHECK(json_valid(limitations_json)), created_at TEXT NOT NULL
);
CREATE TABLE task(
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE, idempotency_key TEXT NOT NULL UNIQUE,
 input_hash TEXT NOT NULL CHECK(length(input_hash)=64), kind TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('queued','running','cancelling','succeeded','failed','cancelled','interrupted')),
 input_json TEXT NOT NULL CHECK(json_valid(input_json)), last_seq INTEGER NOT NULL DEFAULT 0,
 progress_json TEXT CHECK(progress_json IS NULL OR json_valid(progress_json)),
 result_hash TEXT REFERENCES artifact(hash), error_json TEXT CHECK(error_json IS NULL OR json_valid(error_json)),
 parent_id TEXT REFERENCES task(id), retry_of TEXT REFERENCES task(id), created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
 CHECK(state!='succeeded' OR result_hash IS NOT NULL)
);
CREATE TABLE universe(
 id TEXT PRIMARY KEY, name TEXT NOT NULL, snapshot_id TEXT NOT NULL REFERENCES snapshot(id),
 as_of TEXT NOT NULL, membership TEXT NOT NULL CHECK(membership IN ('fixed','dynamic')),
 mode TEXT NOT NULL CHECK(mode IN ('strict','exploratory')),
 rule_json TEXT NOT NULL CHECK(json_valid(rule_json)), version_hash TEXT NOT NULL,
 members_hash TEXT NOT NULL REFERENCES artifact(hash), created_at TEXT NOT NULL
);
CREATE TABLE research_run(
 id TEXT PRIMARY KEY, task_id TEXT NOT NULL UNIQUE REFERENCES task(id),
 snapshot_id TEXT NOT NULL REFERENCES snapshot(id), universe_id TEXT NOT NULL REFERENCES universe(id),
 config_json TEXT NOT NULL CHECK(json_valid(config_json)), config_hash TEXT NOT NULL,
 engine_version TEXT NOT NULL, result_hash TEXT REFERENCES artifact(hash),
 metrics_json TEXT CHECK(metrics_json IS NULL OR json_valid(metrics_json)), created_at TEXT NOT NULL
);
CREATE TABLE holding_version(
 id TEXT PRIMARY KEY, as_of TEXT NOT NULL, input_json TEXT NOT NULL CHECK(json_valid(input_json)),
 hash TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE trade_plan(
 id TEXT PRIMARY KEY, task_id TEXT NOT NULL UNIQUE REFERENCES task(id),
 snapshot_id TEXT NOT NULL REFERENCES snapshot(id), universe_id TEXT NOT NULL REFERENCES universe(id),
 holding_version_id TEXT NOT NULL REFERENCES holding_version(id), as_of TEXT NOT NULL,
 config_json TEXT NOT NULL CHECK(json_valid(config_json)), artifact_hash TEXT NOT NULL REFERENCES artifact(hash), created_at TEXT NOT NULL
);
CREATE TABLE export_record(
 id TEXT PRIMARY KEY, idempotency_key TEXT NOT NULL UNIQUE, input_hash TEXT NOT NULL,
 plan_id TEXT NOT NULL REFERENCES trade_plan(id), destination TEXT NOT NULL,
 sha256 TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE manual_note(
 id TEXT PRIMARY KEY, idempotency_key TEXT NOT NULL UNIQUE, input_hash TEXT NOT NULL,
 plan_id TEXT NOT NULL REFERENCES trade_plan(id), kind TEXT NOT NULL, text TEXT NOT NULL, created_at TEXT NOT NULL
);
CREATE TABLE audit_event(
 id INTEGER PRIMARY KEY, subject_id TEXT NOT NULL, kind TEXT NOT NULL,
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)), created_at TEXT NOT NULL
);
CREATE INDEX task_state_idx ON task(state,created_at);
CREATE INDEX run_snapshot_idx ON research_run(snapshot_id,created_at);
CREATE INDEX audit_subject_idx ON audit_event(subject_id,created_at);
-- 完成实验时同事务更新 task/research_run 与 audit_event；文件必须先就绪。
-- 不可变：artifact、snapshot、universe、holding_version、trade_plan 创建后不更新业务内容。
-- 删除：首版不提供物理删除，被引用产物不清理；仅清理未被登记的临时／孤儿文件。
-- 迁移：schema_version递增，迁移前关闭工作区并备份DB与manifest；旧应用拒绝较新schema。

-- 同步命令与异步入队的统一去重
CREATE TABLE command_receipt(
 idempotency_key TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE,
 input_hash TEXT NOT NULL, method TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('pending','committed','failed')),
 response_json TEXT CHECK(response_json IS NULL OR json_valid(response_json)),
 created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
-- 保存股票池／人工备注等同步操作与committed回执同事务；异步入队回执保存TaskRef。
-- 导出pending意图包含目标路径与预期哈希，恢复时按真实文件核验。
