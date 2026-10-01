//! S11 数据维护场景测试（规格：logos/resources/test/core-S11-test-cases.md）。
//!
//! 覆盖：UT-S11-01～UT-S11-05、ST-S11-01～ST-S11-02。
//! 所有用例经共享 reporter 写入 logos/resources/verify/test-results.jsonl，
//! 断言均为真实计算结果，无 mock 替代。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};

use nautilus_research_domain::{
    Coordinator, CoordinatorConfig, ErrorCode, ImportSource, ImportSpec, PriceBasis, TaskState,
};
use nautilus_research_testkit::case;

// ------------------------------------------------------------------ 测试夹具

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("research-s11-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn write_csv(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    let content = format!(
        "instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n{body}"
    );
    fs::write(&path, content).expect("写入暂存 CSV");
    path.to_string_lossy().to_string()
}

/// 两股各三日固定样本（ST-S11-01）：SYN-A 收 10/11/13，SYN-B 收 20/19/18。
fn fixture_two_symbols(dir: &Path) -> String {
    write_csv(
        dir,
        "staged.csv",
        "SYN-A,2024-01-02,10,10,10,10,100000,1000000\n\
         SYN-A,2024-01-03,10,11,10,11,100000,1100000\n\
         SYN-A,2024-01-04,12,13,12,13,100000,1300000\n\
         SYN-B,2024-01-02,20,20,20,20,80000,1600000\n\
         SYN-B,2024-01-03,19,19,19,19,80000,1520000\n\
         SYN-B,2024-01-04,18,18,18,18,80000,1440000",
    )
}

fn spec_for(path: &str, basis: PriceBasis) -> ImportSpec {
    ImportSpec {
        source: ImportSource::Tickflow,
        paths: vec![path.to_string()],
        symbols: None,
        start: None,
        end: None,
        price_basis: basis,
        auxiliary_kind: None,
    }
}

fn open_coordinator(tag: &str) -> (Coordinator, PathBuf) {
    let ws = temp_workspace(tag);
    let c = Coordinator::open(CoordinatorConfig::new(ws.clone())).expect("打开协调器");
    (c, ws)
}

fn import_ok(c: &Coordinator, key: &str, path: &str, basis: PriceBasis) -> String {
    let task_ref = c
        .import_data(&format!("req-{key}"), key, spec_for(path, basis))
        .expect("ImportData 入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
        .expect("等待任务终态");
    assert_eq!(view.state, TaskState::Succeeded, "导入应成功：{:?}", view.error);
    view.snapshot_id.expect("成功任务必须有 snapshot_id")
}

// ------------------------------------------------------------------ UT 用例

#[test]
fn ut_s11_01_repeat_import_reuses_content() {
    case("UT-S11-01", || {
        let (c, ws) = open_coordinator("ut01");
        let csv = fixture_two_symbols(&ws);
        let snap_a = import_ok(&c, "ut01-a", &csv, PriceBasis::Raw);
        let artifacts_after_first = c
            .list_snapshots(Some(500), None)
            .expect("列出快照")
            .items
            .len();

        // 同一原始日线重复导入（不同幂等键、相同内容）
        let snap_b = import_ok(&c, "ut01-b", &csv, PriceBasis::Raw);

        assert_eq!(snap_a, snap_b, "相同内容必须复用同一快照");
        let page = c.list_snapshots(Some(500), None).expect("列出快照");
        assert_eq!(page.items.len(), artifacts_after_first, "快照记录数不变");
        assert_eq!(page.items.len(), 1, "唯一标的/交易日/口径记录不重复");
        let hash_a = page.items[0].manifest_hash.clone();
        let quotes = c
            .read_snapshot_quotes(&snap_a, Some(500), None)
            .expect("读取行情");
        assert_eq!(quotes.total, 6, "行情记录数不变");
        assert_eq!(quotes.manifest_hash, hash_a, "manifest 内容哈希相同");
    });
}

#[test]
fn ut_s11_02_basis_partitions_isolated() {
    case("UT-S11-02", || {
        let (c, ws) = open_coordinator("ut02");
        let raw_csv = write_csv(&ws, "raw.csv", "SYN-A,2024-01-02,10,10,10,10,100000,1000000");
        let qfq_csv = write_csv(&ws, "qfq.csv", "SYN-A,2024-01-02,11,11,11,11,100000,1100000");

        let snap_raw = import_ok(&c, "ut02-raw", &raw_csv, PriceBasis::Raw);
        let snap_qfq = import_ok(&c, "ut02-qfq", &qfq_csv, PriceBasis::Qfq);

        assert_ne!(snap_raw, snap_qfq, "两个口径是两个快照");
        let page = c.list_snapshots(Some(500), None).expect("列出快照");
        assert_eq!(page.items.len(), 2, "保存两个口径分区");

        let raw = c
            .read_snapshot_quotes(&snap_raw, Some(10), None)
            .expect("读取 raw 行情");
        assert_eq!(raw.rows.len(), 1);
        assert_eq!(raw.rows[0].close.to_string(), "10.0000", "raw 分区不被 qfq 覆盖");
        let qfq = c
            .read_snapshot_quotes(&snap_qfq, Some(10), None)
            .expect("读取 qfq 行情");
        assert_eq!(qfq.rows[0].close.to_string(), "11.0000", "qfq 独立保存");
    });
}

#[test]
fn ut_s11_03_crash_before_commit_leaves_no_partial() {
    case("UT-S11-03", || {
        let (c, ws) = open_coordinator("ut03");
        let good = fixture_two_symbols(&ws);
        let snap_a = import_ok(&c, "ut03-a", &good, PriceBasis::Raw);

        // 分区完成后、DB 提交前注入崩溃
        c.import_hooks()
            .crash_after_partition
            .store(true, Ordering::SeqCst);
        let crash_csv = write_csv(&ws, "crash.csv", "SYN-C,2024-01-02,5,5,5,5,1000,5000");
        let task_ref = c
            .import_data("req-ut03-b", "ut03-b", spec_for(&crash_csv, PriceBasis::Raw))
            .expect("入队崩溃任务");
        let view = c
            .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
            .expect("等待终态");
        assert_eq!(view.state, TaskState::Interrupted, "提交前崩溃应标记 interrupted");
        c.import_hooks()
            .crash_after_partition
            .store(false, Ordering::SeqCst);

        let page = c.list_snapshots(Some(500), None).expect("列出快照");
        assert_eq!(page.items.len(), 1, "快照列表无新记录");
        assert_eq!(page.items[0].snapshot_id, snap_a);
        let quotes = c
            .read_snapshot_quotes(&snap_a, Some(500), None)
            .expect("旧快照可读");
        assert_eq!(quotes.total, 6, "旧快照可读");

        let (orphan_objects, orphan_dirs) = c.reclaim_orphans().expect("孤儿回收");
        assert!(orphan_dirs >= 1, "崩溃暂存目录可回收，实际 {orphan_dirs}");
        assert_eq!(orphan_objects, 0, "崩溃发生在对象登记前，无孤儿对象");
        let page = c.list_snapshots(Some(500), None).expect("回收后再查快照");
        assert_eq!(page.items.len(), 1, "回收不影响已提交快照");
    });
}

#[test]
fn ut_s11_04_old_snapshot_immutable_after_revision() {
    case("UT-S11-04", || {
        let (c, ws) = open_coordinator("ut04");
        let csv_a = write_csv(&ws, "a.csv", "SYN-A,2024-01-02,10,10,10,10,100000,1000000");
        let snap_a = import_ok(&c, "ut04-a", &csv_a, PriceBasis::Raw);
        let hash_a = c
            .list_snapshots(Some(10), None)
            .expect("列出快照")
            .items[0]
            .manifest_hash
            .clone();

        // 修改源文件后再次导入（修订产生新版本）
        let csv_b = write_csv(&ws, "b.csv", "SYN-A,2024-01-02,20,20,20,20,100000,2000000");
        let snap_b = import_ok(&c, "ut04-b", &csv_b, PriceBasis::Raw);
        assert_ne!(snap_a, snap_b, "修订源产生新快照");

        let page = c.list_snapshots(Some(500), None).expect("列出快照");
        assert_eq!(page.items.len(), 2);
        assert_eq!(
            page.items.iter().find(|s| s.snapshot_id == snap_a).unwrap().manifest_hash,
            hash_a,
            "A 哈希不变"
        );
        let quotes_a = c.read_snapshot_quotes(&snap_a, Some(10), None).expect("读取 A");
        assert_eq!(quotes_a.rows[0].close.to_string(), "10.0000", "A 查询值不变");
        let quotes_b = c.read_snapshot_quotes(&snap_b, Some(10), None).expect("读取 B");
        assert_eq!(quotes_b.rows[0].close.to_string(), "20.0000");
    });
}

#[test]
fn ut_s11_05_invalid_rows_reject_whole_batch() {
    case("UT-S11-05", || {
        let (c, ws) = open_coordinator("ut05");
        // 非法日期、负成交量、high<low 同批
        let bad = write_csv(
            &ws,
            "bad.csv",
            "SYN-A,2024-13-45,10,10,10,10,100,1000\n\
             SYN-B,2024-01-02,10,10,10,10,-5,1000\n\
             SYN-C,2024-01-02,10,9,11,10,100,1000",
        );
        let task_ref = c
            .import_data("req-ut05-a", "ut05-a", spec_for(&bad, PriceBasis::Raw))
            .expect("入队非法批次");
        let view = c
            .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
            .expect("等待终态");
        assert_eq!(view.state, TaskState::Failed, "非法行必须整批失败");
        let err = view.error.expect("失败必须含错误");
        assert_eq!(err.code, "INVALID_ARGUMENT");
        for line in ["第 1 行", "第 2 行", "第 3 行"] {
            assert!(err.message.contains(line), "错误须含行号 {line}：{}", err.message);
        }

        // 重复键且无裁决来源
        let dup = write_csv(
            &ws,
            "dup.csv",
            "SYN-A,2024-01-02,10,10,10,10,100,1000\n\
             SYN-A,2024-01-02,11,11,11,11,100,1100",
        );
        let task_ref = c
            .import_data("req-ut05-b", "ut05-b", spec_for(&dup, PriceBasis::Raw))
            .expect("入队重复批次");
        let view = c
            .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
            .expect("等待终态");
        assert_eq!(view.state, TaskState::Failed, "重复冲突必须整批失败");
        assert!(view.error.unwrap().message.contains("第 1 行"), "冲突须给出行号");

        let page = c.list_snapshots(Some(500), None).expect("列出快照");
        assert_eq!(page.items.len(), 0, "无部分完整快照");
    });
}

// ------------------------------------------------------------------ ST 用例

#[test]
fn st_s11_01_import_get_task_list_snapshots() {
    case("ST-S11-01", || {
        // 凭证哨兵：协调器日志绝不允许包含该值（凭证只由数据导入子进程读取）
        const SENTINEL: &str = "s11-sentinel-secret-0123456789";
        // edition 2024：set_var 为 unsafe（测试进程内单用例执行，无并发读写该变量）
        unsafe { std::env::set_var("TICKFLOW_API_KEY", SENTINEL) };

        let (c, ws) = open_coordinator("st01");
        let csv = fixture_two_symbols(&ws);
        let task_ref = c
            .import_data("req-st01", "st01", spec_for(&csv, PriceBasis::Raw))
            .expect("ImportData 入队");
        assert_eq!(task_ref.state, TaskState::Queued, "入队返回 queued");

        // GetTask：等待终态并核对序号与产物
        let view = c
            .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
            .expect("GetTask 等待终态");
        assert_eq!(view.state, TaskState::Succeeded);
        assert!(view.last_seq >= 3, "事件序号递增，实际 {}", view.last_seq);
        assert!(view.artifact_hash.is_some(), "成功时 artifact_hash 必需");
        let snapshot_id = view.snapshot_id.expect("成功时 snapshot_id 必需");

        // ListSnapshots：只返回已提交快照
        let page = c.list_snapshots(Some(500), None).expect("ListSnapshots");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].snapshot_id, snapshot_id);
        assert_eq!(page.items[0].as_of, "2024-01-04", "as_of 为最后完整交易日");

        // 6 条行情，分页可读（limit=2 翻满 3 页）
        let mut total = 0u64;
        let mut cursor: Option<String> = None;
        let mut pages = 0;
        loop {
            let page = c
                .read_snapshot_quotes(&snapshot_id, Some(2), cursor.as_deref())
                .expect("分页读取行情");
            total += page.rows.len() as u64;
            pages += 1;
            assert_eq!(page.total, 6, "总数恒为 6");
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(total, 6, "快照有 6 条行情");
        assert_eq!(pages, 3, "limit=2 恰好 3 页");

        // 跨版本游标拒绝
        let stale = format!("{}:2", "0".repeat(64));
        assert!(c
            .read_snapshot_quotes(&snapshot_id, Some(2), Some(&stale))
            .is_err());

        // 日志无凭证
        let logs = c.logs().join("\n");
        assert!(!logs.contains(SENTINEL), "日志泄露凭证：{logs}");
        assert!(logs.contains("已入队"), "日志含任务动作");
    });
}

#[test]
fn st_s11_02_failure_and_cancel_keep_old_snapshot() {
    case("ST-S11-02", || {
        let (c, ws) = open_coordinator("st02");
        let good = fixture_two_symbols(&ws);
        let snap_a = import_ok(&c, "st02-good", &good, PriceBasis::Raw);

        // 在线失败（以不可读暂存文件模拟网络/源失败）：旧快照仍可研究
        let missing = ws.join("missing.csv").to_string_lossy().to_string();
        let fail_ref = c
            .import_data("req-st02-fail", "st02-fail", spec_for(&missing, PriceBasis::Raw))
            .expect("入队失败任务");
        let fail_view = c
            .wait_terminal(&fail_ref.task_id, Duration::from_secs(15))
            .expect("等待失败终态");
        assert_eq!(fail_view.state, TaskState::Failed);
        assert!(fail_view.artifact_hash.is_none(), "失败无产物");

        // 导入取消：注入逐批延迟，任务进行中取消（200 个唯一标的行）
        c.import_hooks().row_delay_ms.store(40, Ordering::SeqCst);
        let big = write_csv(
            &ws,
            "big.csv",
            &(0..200)
                .map(|i| format!("SYN-{i:03},2024-01-02,10,10,10,10,100,1000"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let cancel_ref = c
            .import_data("req-st02-cancel", "st02-cancel", spec_for(&big, PriceBasis::Raw))
            .expect("入队待取消任务");
        std::thread::sleep(Duration::from_millis(150));
        let cancel_view = c.cancel_task(&cancel_ref.task_id).expect("CancelTask");
        assert!(
            matches!(cancel_view.state, TaskState::Cancelling | TaskState::Cancelled),
            "取消受理，实际 {:?}",
            cancel_view.state
        );
        let final_view = c
            .wait_terminal(&cancel_ref.task_id, Duration::from_secs(30))
            .expect("等待取消终态");
        assert_eq!(final_view.state, TaskState::Cancelled, "取消后落 cancelled");
        c.import_hooks().row_delay_ms.store(0, Ordering::SeqCst);

        // 终态重复取消返回 ALREADY_TERMINAL
        let err = c.cancel_task(&cancel_ref.task_id).expect_err("终态不可再取消");
        assert_eq!(err.code, ErrorCode::AlreadyTerminal);

        // 失败/取消产物不出现在可选列表；旧快照仍可研究
        let page = c.list_snapshots(Some(500), None).expect("ListSnapshots");
        assert_eq!(page.items.len(), 1, "只有既有快照可选");
        assert_eq!(page.items[0].snapshot_id, snap_a);
        let quotes = c.read_snapshot_quotes(&snap_a, Some(500), None).expect("旧快照可读");
        assert_eq!(quotes.total, 6);

        // 取消删除未提交暂存文件
        let tmp = ws.join("tmp");
        let remaining = fs::read_dir(&tmp)
            .map(|d| d.count())
            .unwrap_or(0);
        assert_eq!(remaining, 0, "暂存目录已清理：{tmp:?}");
    });
}
