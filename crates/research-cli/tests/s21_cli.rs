//! S21 CLI 研究链路场景测试（规格：logos/resources/test/core-S21-test-cases.md）。
//!
//! UT 以进程内 `run_cli` 驱动（参数解析/幂等/错误码/双输出/凭证红线）；
//! ST-S21-01 全链路同进程串行子命令；ST-S21-02 取消路径覆盖竞争与终态语义。
//! 约定：TaskState serde 为 lowercase（`succeeded`/`cancelled`）。

use std::path::{Path, PathBuf};

use nautilus_research_testkit::case;

/// 进程内驱动一次 CLI 调用，返回 (退出码, stdout, stderr)。
fn cli(args: &[&str]) -> (i32, String, String) {
    let mut argv: Vec<String> = vec!["research".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = nautilus_research_cli::run_cli(argv, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "research-s21-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn write_file(dir: &Path, name: &str, content: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("写入暂存文件");
    path.to_string_lossy().to_string()
}

/// 24 个交易日行情（2023-12-04 起，收盘 10..33 阶梯上行）。
fn rising_bars(code: &str, dir: &Path, name: &str) -> String {
    let dates = [
        "2023-12-04", "2023-12-05", "2023-12-06", "2023-12-07", "2023-12-08",
        "2023-12-11", "2023-12-12", "2023-12-13", "2023-12-14", "2023-12-15",
        "2023-12-18", "2023-12-19", "2023-12-20", "2023-12-21", "2023-12-22",
        "2023-12-25", "2023-12-26", "2023-12-27", "2023-12-28", "2023-12-29",
        "2024-01-02", "2024-01-03", "2024-01-04", "2024-01-05",
    ];
    let mut body = String::from(
        "instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n",
    );
    for (i, d) in dates.iter().enumerate() {
        let close = 10 + i as i32;
        body.push_str(&format!(
            "{code},{d},{close},{close},{close},{close},100000,{close}0000\n"
        ));
    }
    write_file(dir, name, &body)
}

const RULE_ALL_PASS: &str =
    r#"{"op":"and","children":[{"field":"close","op":"gte","value":"0"}]}"#;

/// 零费率成本参数（run_b 仅替换 --commission-rate 制造 cost 差异）。
const COST_ZERO: &[&str] = &[
    "--commission-rate", "0",
    "--min-commission", "0",
    "--sell-tax", "0",
    "--other-fee", "0",
    "--slippage-bps", "0",
    "--participation", "1",
    "--eff-start", "2023-12-01",
    "--eff-end", "2024-02-01",
];

/// `Vec<String>` → `Vec<&str>`（借用视图）。
fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// 从 JSON 输出提取字符串字段。
fn json_field(text: &str, field: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("输出应为可解析 JSON：{e}\n{text}"));
    v.get(field)
        .and_then(|x| x.as_str())
        .unwrap_or_else(|| panic!("缺少字段 {field}：{text}"))
        .to_string()
}

/// 导入一份上行行情并等待成功，返回 snapshot_id。
fn import_snapshot(w: &str, quotes: &str) -> String {
    let (code, out, err) = cli(&[
        "--workspace", w, "--json", "import",
        "--source", "tdx", "--paths", quotes,
        "--price-basis", "raw", "--wait",
    ]);
    assert_eq!(code, 0, "导入失败：{err}");
    json_field(&out, "snapshot_id")
}

/// 全链路共享夹具：导入→预览→保存，返回 (workspace, snapshot_id, universe_id)。
fn setup_chain(tag: &str) -> (PathBuf, String, String) {
    let ws = temp_workspace(tag);
    let w = ws.to_string_lossy().to_string();
    let quotes = rising_bars("SYN-A", &ws, "a.csv");
    let snapshot_id = import_snapshot(&w, &quotes);

    // 预览（--wait）：JSON 输出 {"task": TaskView, "preview": UniversePreview}。
    let (code, out, err) = cli(&[
        "--workspace", &w, "--json", "universe", "preview",
        "--snapshot", &snapshot_id, "--as-of", "2024-01-05",
        "--mode", "strict", "--membership", "fixed", "--rule", RULE_ALL_PASS, "--wait",
    ]);
    assert_eq!(code, 0, "预览失败：{err}");
    let parsed: serde_json::Value =
        serde_json::from_str(out.trim()).expect("预览输出应为 JSON");
    let preview_hash = parsed["preview"]["preview_hash"]
        .as_str().expect("preview_hash").to_string();
    let input_hash = parsed["preview"]["input_hash"]
        .as_str().expect("input_hash").to_string();
    let preview_task = parsed["task"]["task_id"]
        .as_str().expect("preview task_id").to_string();

    // 保存（同步）：--preview-task 需要预览任务 ID。
    let (code, out, err) = cli(&[
        "--workspace", &w, "--json", "universe", "save",
        "--preview-task", &preview_task,
        "--preview-hash", &preview_hash,
        "--input-hash", &input_hash,
        "--name", &format!("池-{tag}"),
    ]);
    assert_eq!(code, 0, "保存失败：{err}");
    let universe_id = json_field(&out, "universe_id");
    (ws, snapshot_id, universe_id)
}

/// 从 snapshots 列表取第一个 snapshot_id。
fn first_snapshot_id(w: &str) -> String {
    let (code, out, err) = cli(&["--workspace", w, "--json", "snapshots"]);
    assert_eq!(code, 0, "查询快照失败：{err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).expect("snapshots JSON");
    v["items"][0]["snapshot_id"].as_str().expect("snapshot_id").to_string()
}

// ------------------------------------------------------------------ UT

/// UT-S21-01：非法参数非零退出码 + 中文诊断，且协调器零调用（不存在的工作区路径未被创建）。
#[test]
fn ut_s21_01_rejects_bad_args_without_coordinator() {
    case("UT-S21-01", || {
        // 工作区路径故意指向不存在的目录：若 CLI 误触协调器会创建它。
        let untouched = temp_workspace("ut01-ws");
        let ws = untouched.join("工作 区不应被创建");
        let w = ws.to_string_lossy().to_string();

        // 1) import 缺 --paths（解析层拒绝）
        let (code, _, err) = cli(&["--workspace", &w, "import", "--source", "tdx"]);
        assert_ne!(code, 0, "缺 --paths 必须非零退出码");
        assert!(err.contains("缺少必填参数 --paths"), "中文诊断：{err}");

        // 2) compare 数量越界（1 个 < 2 与 6 个 > 5，解析层与协调器同口径前置校验）
        let (code, _, err) = cli(&[
            "--workspace", &w, "compare", "--runs", "RUN-ONLY-ONE", "--view", "full",
        ]);
        assert_ne!(code, 0, "compare 单运行必须非零退出码");
        assert!(err.contains("2..5"), "数量诊断：{err}");
        let (code, _, err) = cli(&[
            "--workspace", &w, "compare",
            "--runs", "R1,R2,R3,R4,R5,R6", "--view", "full",
        ]);
        assert_ne!(code, 0, "compare 六运行必须非零退出码");
        assert!(err.contains("2..5"), "数量诊断：{err}");

        // 3) 未知子命令 → 完整中文用法
        let (code, _, err) = cli(&["--workspace", &w, "未知的子命令"]);
        assert_ne!(code, 0);
        assert!(err.contains("research —— A 股研究桌面命令行"), "用法总览：{err}");
        assert!(err.contains("退出码：0 成功；3 业务拒绝（含用法错误）；4 环境错误"));

        // 协调器零调用探针：工作区子目录从未被创建。
        assert!(
            !ws.exists(),
            "非法参数不得触达协调器（工作区 {ws:?} 不应被创建）"
        );
        std::fs::remove_dir_all(&untouched).ok();
    });
}

/// UT-S21-02：同键同载荷返回原任务（幂等回执，退出码 0）；同键不同载荷 IDEMPOTENCY_CONFLICT（退出码 3）。
#[test]
fn ut_s21_02_idempotent_replay_and_conflict() {
    case("UT-S21-02", || {
        let ws = temp_workspace("ut02");
        let w = ws.to_string_lossy().to_string();
        let quotes_a = rising_bars("SYN-A", &ws, "a.csv");

        let import_args = |path: &str| -> Vec<String> {
            vec![
                "--workspace".into(), w.clone(), "--json".into(), "import".into(),
                "--source".into(), "tdx".into(),
                "--paths".into(), path.into(),
                "--price-basis".into(), "raw".into(),
                "--wait".into(), "--idempotency-key".into(), "ut02-key".into(),
            ]
        };
        let a1 = import_args(&quotes_a);
        let (code1, out1, err1) = cli(&refs(&a1));
        assert_eq!(code1, 0, "首次导入失败：{err1}");
        let task1 = json_field(&out1, "task_id");

        // 同键同载荷重放 → 原 TaskRef。
        let a2 = import_args(&quotes_a);
        let (code2, out2, err2) = cli(&refs(&a2));
        assert_eq!(code2, 0, "幂等重放必须成功：{err2}");
        assert_eq!(json_field(&out2, "task_id"), task1, "同键同载荷返回原任务");

        // 同键不同载荷 → IDEMPOTENCY_CONFLICT。
        let other = rising_bars("SYN-B", &ws, "b.csv");
        let a3 = import_args(&other);
        let (code3, _, err3) = cli(&refs(&a3));
        assert_eq!(code3, 3, "IDEMPOTENCY_CONFLICT 退出码 3：{err3}");
        assert!(err3.contains("IDEMPOTENCY_CONFLICT"), "错误码透传：{err3}");
    });
}

/// UT-S21-03：run --wait 引用不存在快照 → 退出码 3、错误码透传、stdout 无部分产物。
#[test]
fn ut_s21_03_run_missing_snapshot_fails_with_code_3() {
    case("UT-S21-03", || {
        let ws = temp_workspace("ut03");
        let w = ws.to_string_lossy().to_string();
        let mut args: Vec<String> = [
            "--workspace", &w, "--json",
            "run", "submit",
            "--snapshot", "SNAP-404",
            "--universe", "UNI-404",
            "--template", "ema", "--fast", "1", "--slow", "2",
            "--top-k", "1", "--rebalance", "daily",
            "--start", "2023-12-04", "--end", "2024-01-05",
            "--capital", "20000",
            "--rules-hash", &"0".repeat(64),
            "--mode", "strict",
            "--wait",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        args.extend(COST_ZERO.iter().map(|s| s.to_string()));
        let (code, out, err) = cli(&args.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(code, 3, "不存在快照应业务拒绝：{out}{err}");
        assert!(err.contains("NOT_FOUND"), "错误码透传：{err}");
        assert!(
            out.trim().is_empty(),
            "失败命令 stdout 不得有部分产物：{out}"
        );
    });
}

/// UT-S21-04：--json 可解析且与表格同源；查询类命令零写入（objects/ 文件集不变）。
#[test]
fn ut_s21_04_dual_output_and_readonly_query() {
    case("UT-S21-04", || {
        let (ws, _sid, _uid) = setup_chain("ut04");
        let w = ws.to_string_lossy().to_string();

        let objects_snapshot = || -> Vec<String> {
            let mut files: Vec<String> = walk(&ws.join("objects"));
            files.sort();
            files
        };
        let before = objects_snapshot();

        // JSON 形态可解析，快照数与夹具一致。
        let (code, out, err) = cli(&["--workspace", &w, "--json", "snapshots"]);
        assert_eq!(code, 0, "查询失败：{err}");
        let parsed: serde_json::Value =
            serde_json::from_str(out.trim()).expect("JSON 输出可解析");
        assert_eq!(parsed["items"].as_array().expect("items 数组").len(), 1);

        // 表格形态与 JSON 同源（同一份快照记录）。
        let (code, out2, err2) = cli(&["--workspace", &w, "snapshots"]);
        assert_eq!(code, 0, "表格查询失败：{err2}");
        assert!(out2.contains("快照数: 1"), "表格输出：{out2}");

        assert_eq!(before, objects_snapshot(), "查询类命令零写入（objects/ 不变）");
    });
}

fn walk(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                files.extend(walk(&p));
            } else {
                files.push(p.to_string_lossy().to_string());
            }
        }
    }
    files
}

/// UT-S21-05：工作区路径含中文与空格，导入与查询正常。
#[test]
fn ut_s21_05_workspace_with_chinese_and_spaces() {
    case("UT-S21-05", || {
        let base = temp_workspace("ut05-base");
        let ws = base.join("研究 桌面/测试 工作区");
        std::fs::create_dir_all(&ws).expect("创建中文空格路径");
        let quotes = rising_bars("SYN-A", &ws, "a.csv");
        let w = ws.to_string_lossy().to_string();

        let snapshot_id = import_snapshot(&w, &quotes);
        assert!(snapshot_id.len() >= 32, "快照 ID 正常：{snapshot_id}");

        let (code, out2, _) = cli(&["--workspace", &w, "snapshots"]);
        assert_eq!(code, 0);
        assert!(out2.contains("快照数: 1"), "表格转义输出：{out2}");
    });
}

/// UT-S21-06：注入凭证探针后执行导入与查询，输出不含变量名与探针值（凭证红线）。
#[test]
fn ut_s21_06_credential_probe_never_leaks() {
    case("UT-S21-06", || {
        // edition 2024：set_var 为 unsafe。研究代码不读该变量（红线），
        // 注入只用于验证"即使环境里有探针值也不会出现在任何输出中"。
        unsafe { std::env::set_var("TICKFLOW_API_KEY", "probe-secret-ut06-xyz") };

        let (ws, _sid, _uid) = setup_chain("ut06");
        let w = ws.to_string_lossy().to_string();
        let quotes = rising_bars("SYN-A", &ws, "a2.csv");

        // tickflow 源导入（进程内执行器不读凭证；业务成败不作断言，红线为主）。
        let (code1, out1, err1) = cli(&[
            "--workspace", &w, "--json", "import",
            "--source", "tickflow", "--paths", &quotes,
            "--price-basis", "raw", "--wait", "--wait-timeout", "30",
        ]);
        let (code2, out2, err2) = cli(&["--workspace", &w, "--json", "snapshots"]);
        for (code, out, err) in [(&code1, &out1, &err1), (&code2, &out2, &err2)] {
            let combined = format!("{out}{err}");
            assert!(!combined.contains("TICKFLOW_API_KEY"), "输出泄漏变量名：{combined}");
            assert!(!combined.contains("probe-secret-ut06-xyz"), "输出泄漏探针值");
            assert!(*code == 0 || *code == 3, "退出码契约：{code}");
        }
    });
}

// ------------------------------------------------------------------ ST

/// ST-S21-01：CLI 全链路——导入→池→两次运行→比较→计划生成/导出/备注，逐步退出码 0。
#[test]
fn st_s21_01_full_pipeline_via_cli() {
    case("ST-S21-01", || {
        let (ws, _sid, uid) = setup_chain("st01");
        let w = ws.to_string_lossy().to_string();
        let sid = first_snapshot_id(&w);

        // 运行 A（零费率，EMA 1/2）与运行 B（非零佣金，EMA 2/2）——差异显式列出。
        let submit = |rate: &str, fast: &str| -> String {
            let mut args: Vec<String> = [
                "--workspace", &w, "--json",
                "run", "submit",
                "--snapshot", &sid,
                "--universe", &uid,
                "--template", "ema",
                "--fast", fast, "--slow", "2",
                "--top-k", "1", "--rebalance", "daily",
                "--start", "2023-12-04", "--end", "2024-01-05",
                "--capital", "20000",
                "--rules-hash", &"0".repeat(64),
                "--mode", "strict",
                "--wait",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            args.push("--commission-rate".into());
            args.push(rate.into());
            args.extend(COST_ZERO.iter().skip(2).map(|s| s.to_string()));
            let (code, out, err) = cli(&args.iter().map(String::as_str).collect::<Vec<_>>());
            assert_eq!(code, 0, "运行失败（rate={rate} fast={fast}）：{err}");
            let v: serde_json::Value = serde_json::from_str(out.trim()).expect("运行 JSON");
            assert_eq!(v["state"], "succeeded", "运行应成功：{out}");
            v["task_id"].as_str().expect("task_id").to_string()
        };
        let run_a = submit("0", "1");
        let run_b = submit("0.001", "1");

        // 比较：完整视图 + 双方指标行 + 费用差异显式列出。
        let (code, out, err) = cli(&[
            "--workspace", &w, "--json", "compare",
            "--runs", &format!("{run_a},{run_b}"), "--view", "full",
        ]);
        assert_eq!(code, 0, "比较失败：{err}");
        let cmp: serde_json::Value = serde_json::from_str(out.trim()).expect("比较 JSON");
        assert_eq!(cmp["runs"].as_array().expect("runs").len(), 2, "双方指标行：{out}");
        assert!(
            cmp["differences"]
                .as_array().expect("differences")
                .iter().any(|d| d["kind"] == "cost"),
            "费用差异显式列出：{out}"
        );

        // 计划生成（手工持仓）。
        let holdings = serde_json::json!({
            "as_of": "2024-01-05",
            "cash_cny": "10000",
            "positions": [
                {"instrument_id": "SYN-A", "quantity": 100, "sellable_quantity": 100}
            ],
            "total_assets_cny": "20000"
        })
        .to_string();
        let mut plan_args: Vec<String> = [
            "--workspace", &w, "--json",
            "plan", "generate",
            "--snapshot", &sid,
            "--universe", &uid,
            "--as-of", "2024-01-05",
            "--template", "ema", "--fast", "1", "--slow", "2",
            "--top-k", "1", "--rebalance", "daily",
            "--rules-hash", &"0".repeat(64),
            "--mode", "strict",
            "--holdings", &holdings,
            "--wait",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        plan_args.extend(COST_ZERO.iter().map(|s| s.to_string()));
        let (code, out, err) = cli(&plan_args.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(code, 0, "计划生成失败：{err}");
        let plan_view: serde_json::Value = serde_json::from_str(out.trim()).expect("计划 JSON");
        assert_eq!(plan_view["state"], "succeeded", "计划应成功：{out}");
        let plan_id = plan_view["task_id"].as_str().expect("plan task_id").to_string();

        // 导出：CSV 落盘且 sha256 与回执一致（中文空格目录一并覆盖）。
        let destination = ws.join("导出 目录/计划.csv");
        let (code, out, err) = cli(&[
            "--workspace", &w, "--json", "plan", "export",
            "--plan-id", &plan_id,
            "--destination", &destination.to_string_lossy(),
        ]);
        assert_eq!(code, 0, "导出失败：{err}");
        let receipt: serde_json::Value = serde_json::from_str(out.trim()).expect("回执 JSON");
        let sha = receipt["sha256"].as_str().expect("sha256");
        assert!(destination.exists(), "CSV 落盘");
        let bytes = std::fs::read(&destination).expect("读取导出文件");
        assert_eq!(
            nautilus_research_domain::hash::sha256_hex(&bytes),
            sha,
            "导出哈希与回执一致"
        );

        // 备注。
        let (code, out, err) = cli(&[
            "--workspace", &w, "--json", "plan", "note",
            "--plan-id", &plan_id, "--text", "已人工核对", "--kind", "备注",
        ]);
        assert_eq!(code, 0, "备注失败：{err}");
        assert!(out.contains("note_id"), "备注回执：{out}");
    });
}

/// ST-S21-02：跨命令取消语义（CLI 单命令进程形态）。
///
/// 架构口径：CLI 每条子命令独立开关协调器（单工作区单活跃协调器）。
/// submit 不带 --wait 退出后，在下一条命令 open 时由重启扫描把非终态任务
/// 诚实标注 interrupted（崩溃恢复语义）；因此跨命令 cancel 的确定性行为是
/// ALREADY_TERMINAL（退出码 3）。同进程取消语义由协调器场景测试覆盖
/// （crates/research-domain/tests），GUI 长驻协调器形态下取消直达（批次 L4）。
#[test]
fn st_s21_02_cancel_semantics_via_subcommand() {
    case("ST-S21-02", || {
        let (ws, _sid, uid) = setup_chain("st02");
        let w = ws.to_string_lossy().to_string();
        let sid = first_snapshot_id(&w);

        // 提交（不等待）拿 task_id；命令退出即协调器生命周期结束。
        let mut args: Vec<String> = [
            "--workspace", &w, "--json",
            "run", "submit",
            "--snapshot", &sid,
            "--universe", &uid,
            "--template", "ema", "--fast", "1", "--slow", "2",
            "--top-k", "1", "--rebalance", "daily",
            "--start", "2023-12-04", "--end", "2024-01-05",
            "--capital", "20000",
            "--rules-hash", &"0".repeat(64),
            "--mode", "strict",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        args.extend(COST_ZERO.iter().map(|s| s.to_string()));
        let (code, out, err) = cli(&refs(&args));
        assert_eq!(code, 0, "提交失败：{err}");
        let task_id = json_field(&out, "task_id");

        // 跨命令 cancel：任务此时必为终态——
        //   submit 进程退出前完成事件已被处理 → succeeded；
        //   否则下次 open 的重启扫描 → interrupted。
        // 两种终态下 cancel 都确定性地返回 ALREADY_TERMINAL（退出码 3）。
        let (code, _, err) = cli(&["--workspace", &w, "run", "cancel", "--task", &task_id]);
        assert_eq!(code, 3, "跨命令取消必须确定性 ALREADY_TERMINAL：{code} {err}");
        assert!(err.contains("ALREADY_TERMINAL"), "错误码透传：{err}");

        // 等待终态：立即返回，中断无部分产物。
        let (code, out, err) = cli(&[
            "--workspace", &w, "--json", "task", "wait",
            "--task", &task_id, "--timeout", "30",
        ]);
        assert_eq!(code, 0, "等待终态失败：{err}");
        let v: serde_json::Value = serde_json::from_str(out.trim()).expect("终态 JSON");
        let state = v["state"].as_str().expect("state").to_string();
        assert!(
            state == "succeeded" || state == "interrupted",
            "终态与提交竞态一致（succeeded 或诚实中断）：{state}"
        );
        if state == "interrupted" {
            assert!(
                v["artifact_hash"].is_null(),
                "中断无已提交产物：{out}"
            );
        }

        // 再次 show：终态一致（幂等视图）。
        let (code, out2, err2) =
            cli(&["--workspace", &w, "--json", "task", "get", "--task", &task_id]);
        assert_eq!(code, 0, "查询失败：{err2}");
        assert_eq!(json_field(&out2, "state"), state, "再次 show 终态一致");
    });
}
