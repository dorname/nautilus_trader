//! S12 股票池筛选场景测试（规格：logos/resources/test/core-S12-test-cases.md）。
//!
//! 覆盖：UT-S12-01～UT-S12-05、ST-S12-01～ST-S12-02。
//! 所有用例经共享 reporter 写入 logos/resources/verify/test-results.jsonl，
//! 断言均为真实计算结果，无 mock 替代。

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use nautilus_research_domain::{
    hash::hash_canonical,
    protocol::{Membership, MissingPolicy, RowsTable, SaveUniverseSpec, UniverseMode, UniverseSpec},
    universe::{eval_group, precheck_run_universe, Condition, EvalContext, OrGroup, OrOp, RuleChild, RuleField, RuleGroup, RuleOp, Tri, AndOp},
    Coordinator, CoordinatorConfig, ErrorCode, ImportSource, ImportSpec, PriceBasis, QuoteRow, TaskState,
};
use nautilus_research_testkit::case;

// ------------------------------------------------------------------ 测试夹具

fn temp_workspace(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("research-s12-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("创建临时工作区");
    dir
}

fn write_file(dir: &Path, name: &str, content: &str) -> String {
    let path = dir.join(name);
    fs::write(&path, content).expect("写入暂存文件");
    path.to_string_lossy().to_string()
}

fn quotes_csv(dir: &Path, name: &str, body: &str) -> String {
    write_file(
        dir,
        name,
        &format!("instrument_id,trade_date,open,high,low,close,volume_shares,amount_cny\n{body}"),
    )
}

fn master_csv(dir: &Path, name: &str, body: &str) -> String {
    write_file(dir, name, &format!("instrument_id,board,listed_date,delisted_date\n{body}"))
}

fn financial_csv(dir: &Path, name: &str, body: &str) -> String {
    write_file(
        dir,
        name,
        &format!("instrument_id,period_end,available_at,revision,roe,eps,bps,net_profit,revenue_growth,debt_ratio\n{body}"),
    )
}

fn open_coordinator(tag: &str) -> (Coordinator, PathBuf) {
    let ws = temp_workspace(tag);
    let c = Coordinator::open(CoordinatorConfig::new(ws.clone())).expect("打开协调器");
    (c, ws)
}

fn import_ok(c: &Coordinator, key: &str, paths: Vec<String>) -> String {
    let spec = ImportSpec {
        source: ImportSource::Auxiliary,
        paths,
        symbols: None,
        start: None,
        end: None,
        price_basis: PriceBasis::Raw,
        auxiliary_kind: None,
    };
    let task_ref = c
        .import_data(&format!("req-{key}"), key, spec)
        .expect("ImportData 入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
        .expect("等待导入终态");
    assert_eq!(view.state, TaskState::Succeeded, "导入应成功：{:?}", view.error);
    view.snapshot_id.expect("成功任务必须有 snapshot_id")
}

/// 单条件规则（根 AND 只包一个条件）。
fn rule_single(c: Condition) -> RuleGroup {
    RuleGroup { op: AndOp::And, children: vec![RuleChild::Cond(c)] }
}

fn cond(field: RuleField, op: RuleOp, value: &str) -> Condition {
    Condition {
        field,
        op,
        value: Some(value.to_string()),
        values: None,
        window: None,
    }
}

fn universe_spec(snapshot_id: &str, as_of: &str, rule: RuleGroup) -> UniverseSpec {
    UniverseSpec {
        snapshot_id: snapshot_id.to_string(),
        as_of: as_of.to_string(),
        mode: UniverseMode::Strict,
        membership: Membership::Fixed,
        rule,
        missing_policy: None,
    }
}

/// 预览并等待成功，返回 (task_id, 预览摘要)。
fn preview_ok(
    c: &Coordinator,
    key: &str,
    spec: UniverseSpec,
) -> (String, nautilus_research_domain::protocol::UniversePreview) {
    let task_ref = c
        .preview_universe(&format!("req-{key}"), key, spec)
        .expect("PreviewUniverse 入队");
    let view = c
        .wait_terminal(&task_ref.task_id, Duration::from_secs(15))
        .expect("等待预览终态");
    assert_eq!(view.state, TaskState::Succeeded, "预览应成功：{:?}", view.error);
    let summary = c.universe_preview(&task_ref.task_id).expect("预览摘要");
    (task_ref.task_id, summary)
}

// ------------------------------------------------------------------ UT 用例

#[test]
fn ut_s12_01_future_report_not_visible() {
    case("UT-S12-01", || {
        let (c, ws) = open_coordinator("ut01");
        // 报告期 2023-12-31、公告 2024-04-01；as_of=2024-03-01 不能用未来报告
        let master = master_csv(&ws, "master.csv", "SYN-A,main,2020-01-01,");
        let fin = financial_csv(&ws, "fin.csv", "SYN-A,2023-12-31,2024-04-01,0,8,,,,,");
        let snap = import_ok(&c, "ut12-01", vec![master, fin]);

        let rule = rule_single(cond(RuleField::Roe, RuleOp::Gte, "5"));
        let (task_id, summary) = preview_ok(&c, "ut12-01-p", universe_spec(&snap, "2024-03-01", rule));
        assert_eq!(summary.unknown, 1, "只有未公告报告时条件必须为未知");
        assert_eq!(summary.pass, 0, "不得用未来报告判命中");

        let page = c
            .query_rows(&task_id, RowsTable::Unknown, Some(10), None)
            .expect("查询未知桶");
        assert_eq!(page.rows.len(), 1);
        assert!(
            member_of(&page).reasons.iter().any(|r| r.contains("尚无已公告报告")),
            "原因需说明时点可见性：{:?}",
            member_of(&page).reasons
        );
    });
}

#[test]
fn ut_s12_02_delisted_stock_stays_in_historical_pool() {
    case("UT-S12-02", || {
        let (c, ws) = open_coordinator("ut02");
        // A 于 2024-06-01 退市；历史筛选日 2024-03-01 仍在册
        let master = master_csv(&ws, "master.csv", "SYN-A,main,2020-01-01,2024-06-01");
        let snap = import_ok(&c, "ut12-02", vec![master]);

        let rule = RuleGroup {
            op: AndOp::And,
            children: vec![RuleChild::Cond(Condition {
                field: RuleField::Board,
                op: RuleOp::In,
                value: None,
                values: Some(vec!["main".to_string()]),
                window: None,
            })],
        };
        let (_t, summary) = preview_ok(&c, "ut12-02-p", universe_spec(&snap, "2024-03-01", rule.clone()));
        assert_eq!(summary.pass, 1, "历史筛选日 A 仍在册，不能按今天主档删除");

        // 退市日后的口径：A 不再入选
        let (_t2, summary2) = preview_ok(&c, "ut12-02-p2", universe_spec(&snap, "2024-07-01", rule));
        assert_eq!(summary2.pass, 0, "退市日后 A 不应入选");
    });
}

#[test]
fn ut_s12_03_three_state_composition() {
    case("UT-S12-03", || {
        // 纯函数级：构造上下文直接评估组合语义
        let quote = |close: &str| QuoteRow {
            instrument_id: "SYN-A".to_string(),
            trade_date: "2024-01-04".to_string(),
            open: close.parse().unwrap(),
            high: close.parse().unwrap(),
            low: close.parse().unwrap(),
            close: close.parse().unwrap(),
            volume_shares: 100,
            amount_cny: None,
        };
        let mut quotes = std::collections::BTreeMap::new();
        quotes.insert("SYN-A".to_string(), vec![quote("10")]);
        let ctx = EvalContext {
            as_of: "2024-01-04",
            master: None,
            financial: None, // 财务缺失 → ROE 条件未知
            quotes: Some(&quotes),
            ignore_unknown_conditions: false,
        };

        // OR(命中, 未知) → 命中
        let or_rule = RuleGroup {
            op: AndOp::And,
            children: vec![RuleChild::Or(OrGroup {
                op: OrOp::Or,
                children: vec![
                    cond(RuleField::Close, RuleOp::Gte, "10"),
                    cond(RuleField::Roe, RuleOp::Gte, "5"),
                ],
            })],
        };
        let (v, _, _) = eval_group(&or_rule, "SYN-A", &ctx);
        assert_eq!(v, Tri::Hit, "OR 一命中一未知 → 命中");

        // AND 含不满足 → 不满足
        let and_miss = RuleGroup {
            op: AndOp::And,
            children: vec![
                RuleChild::Cond(cond(RuleField::Close, RuleOp::Gte, "10")),
                RuleChild::Cond(cond(RuleField::Close, RuleOp::Lte, "9")),
            ],
        };
        let (v, reasons, _) = eval_group(&and_miss, "SYN-A", &ctx);
        assert_eq!(v, Tri::Miss, "AND 有不满足 → 不满足");
        assert!(!reasons.is_empty(), "原因必须保留");

        // 全部未知 → 未知
        let all_unknown = rule_single(cond(RuleField::Roe, RuleOp::Gte, "5"));
        let (v, _, _) = eval_group(&all_unknown, "SYN-A", &ctx);
        assert_eq!(v, Tri::Unknown, "全部未知 → 未知");

        // 模式约束：ignore_condition 只能用于探索模式（契约 RuleAST/missing_policy）
        let mut spec = universe_spec("snap", "2024-01-04", rule_single(cond(RuleField::Close, RuleOp::Gte, "10")));
        spec.missing_policy = Some(MissingPolicy::IgnoreCondition);
        let err = spec.validate().expect_err("严格模式禁止 ignore_condition");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        spec.mode = UniverseMode::Exploratory;
        spec.validate().expect("探索模式允许 ignore_condition");
    });
}

#[test]
fn ut_s12_04_fixed_pool_formed_after_run_start_rejected() {
    case("UT-S12-04", || {
        // 固定池形成日晚于回测开始日 → 严格预检拒绝并指出形成日
        let err = precheck_run_universe("2024-01-10", "fixed", "2024-01-02", "strict")
            .expect_err("严格模式必须拒绝");
        assert_eq!(err.code, ErrorCode::InvalidArgument);
        assert!(err.message.contains("2024-01-10"), "错误需指出形成日：{}", err.message);

        // 动态池不受此约束（回测逐日重算）；探索模式降级允许
        precheck_run_universe("2024-01-10", "dynamic", "2024-01-02", "strict").expect("动态池不受形成日约束");
        precheck_run_universe("2024-01-10", "fixed", "2024-01-02", "exploratory").expect("探索模式允许");
        precheck_run_universe("2024-01-01", "fixed", "2024-01-02", "strict").expect("形成日早于开始日可通过");
    });
}

#[test]
fn ut_s12_05_missing_amount_is_unknown_not_substituted() {
    case("UT-S12-05", || {
        let (c, ws) = open_coordinator("ut05");
        // 两日 close/volume 完整，其中一日成交额缺失；若用 close×volume 替代将得到 1,000,000
        let quotes = quotes_csv(
            &ws,
            "quotes.csv",
            "SYN-A,2024-01-03,10,10,10,10,100000,\n\
             SYN-A,2024-01-04,10,10,10,10,100000,1000000",
        );
        let snap = import_ok(&c, "ut12-05", vec![quotes]);

        let mut cnd = cond(RuleField::AvgAmount, RuleOp::Gte, "1000000");
        cnd.window = Some(2);
        let (task_id, summary) = preview_ok(&c, "ut12-05-p", universe_spec(&snap, "2024-01-04", rule_single(cnd)));
        assert_eq!(summary.unknown, 1, "成交额缺失必须为未知");
        assert_eq!(summary.pass, 0, "绝不用 close×volume 替代成交额");

        let page = c
            .query_rows(&task_id, RowsTable::Unknown, Some(10), None)
            .expect("查询未知桶");
        assert!(
            member_of(&page).reasons.iter().any(|r| r.contains("成交额缺失")),
            "原因需说明成交额缺失：{:?}",
            member_of(&page).reasons
        );
    });
}

// ------------------------------------------------------------------ ST 用例

#[test]
fn st_s12_01_preview_save_query_three_states() {
    case("ST-S12-01", || {
        let (c, ws) = open_coordinator("st01");
        // F-POOL：A 满足价格条件（收 10），B 不满足（收 5），C 无行情（状态未知）
        let quotes = quotes_csv(
            &ws,
            "quotes.csv",
            "SYN-A,2024-01-02,10,10,10,10,100000,1000000\n\
             SYN-A,2024-01-03,10,10,10,10,100000,1000000\n\
             SYN-A,2024-01-04,10,10,10,10,100000,1000000\n\
             SYN-B,2024-01-02,5,5,5,5,100000,500000\n\
             SYN-B,2024-01-03,5,5,5,5,100000,500000\n\
             SYN-B,2024-01-04,5,5,5,5,100000,500000",
        );
        let master = master_csv(
            &ws,
            "master.csv",
            "SYN-A,main,2020-01-01,\nSYN-B,main,2020-01-01,\nSYN-C,gem,2021-06-01,",
        );
        let snap = import_ok(&c, "st12-01", vec![quotes, master]);

        let spec = universe_spec(&snap, "2024-01-04", rule_single(cond(RuleField::Close, RuleOp::Gte, "10")));
        let (task_id, summary) = preview_ok(&c, "st12-01-p", spec.clone());
        assert_eq!((summary.pass, summary.exclude, summary.unknown), (1, 1, 1), "三态各一");

        // 保存：哈希必须与预览一致
        let saved = c
            .save_universe(
                "req-st12-01-s",
                "st12-01-s",
                SaveUniverseSpec {
                    preview_task_id: task_id.clone(),
                    preview_hash: summary.preview_hash.clone(),
                    input_hash: summary.input_hash.clone(),
                    name: "测试池A".to_string(),
                },
            )
            .expect("SaveUniverse");
        assert_eq!(saved.count, 1, "成员数为命中数");
        assert_eq!(saved.rule_hash, hash_canonical(&spec.rule), "规则哈希一致");

        // 保存哈希与预览一致（members 产物即预览产物）
        let universe = c.get_universe(&saved.universe_id).expect("读取股票池").expect("股票池存在");
        assert_eq!(universe.members_hash, summary.preview_hash, "保存哈希与预览一致");

        // 分页查询三桶
        let members = c.query_rows(&task_id, RowsTable::Members, Some(500), None).expect("查成员");
        assert_eq!(members.total, 1);
        assert_eq!(member_of(&members).instrument_id, "SYN-A");
        let excluded = c.query_rows(&task_id, RowsTable::Excluded, Some(500), None).expect("查排除");
        assert_eq!(excluded.total, 1);
        assert_eq!(member_of(&excluded).instrument_id, "SYN-B");
        assert!(!member_of(&excluded).reasons.is_empty(), "排除需带原因");
        let unknown = c.query_rows(&task_id, RowsTable::Unknown, Some(500), None).expect("查未知");
        assert_eq!(unknown.total, 1);
        assert_eq!(member_of(&unknown).instrument_id, "SYN-C");

        // 游标绑定对象哈希：跨对象偏移拒绝
        let err = c
            .query_rows(&task_id, RowsTable::Members, Some(1), Some("deadbeef:0"))
            .expect_err("跨版本游标必须拒绝");
        assert_eq!(err.code, ErrorCode::InvalidArgument);

        // 幂等重放：同键同载荷返回同一股票池
        let saved2 = c
            .save_universe(
                "req-st12-01-s",
                "st12-01-s",
                SaveUniverseSpec {
                    preview_task_id: task_id,
                    preview_hash: summary.preview_hash,
                    input_hash: summary.input_hash,
                    name: "测试池A".to_string(),
                },
            )
            .expect("幂等重放");
        assert_eq!(saved2.universe_id, saved.universe_id, "幂等重放返回原股票池");
    });
}

#[test]
fn st_s12_02_stale_preview_rejected() {
    case("ST-S12-02", || {
        let (c, ws) = open_coordinator("st02");
        let quotes = quotes_csv(
            &ws,
            "quotes.csv",
            "SYN-A,2024-01-04,10,10,10,10,100000,1000000",
        );
        let snap = import_ok(&c, "st12-02", vec![quotes]);

        // 预览 R1（close ≥ 10）并保存成功
        let spec1 = universe_spec(&snap, "2024-01-04", rule_single(cond(RuleField::Close, RuleOp::Gte, "10")));
        let (task1, pv1) = preview_ok(&c, "st12-02-p1", spec1);
        let saved = c
            .save_universe(
                "req-st12-02-s1",
                "st12-02-s1",
                SaveUniverseSpec {
                    preview_task_id: task1.clone(),
                    preview_hash: pv1.preview_hash.clone(),
                    input_hash: pv1.input_hash.clone(),
                    name: "原始池".to_string(),
                },
            )
            .expect("首次保存");
        let before = c.get_universe(&saved.universe_id).expect("读取").expect("存在");

        // 修改规则为 R2（close ≥ 20），仍用旧 preview_hash/input_hash 组合保存
        let spec2 = universe_spec(&snap, "2024-01-04", rule_single(cond(RuleField::Close, RuleOp::Gte, "20")));
        let err = c
            .save_universe(
                "req-st12-02-s2",
                "st12-02-s2",
                SaveUniverseSpec {
                    preview_task_id: task1.clone(),
                    preview_hash: pv1.preview_hash.clone(),
                    input_hash: hash_canonical(&spec2),
                    name: "篡改池".to_string(),
                },
            )
            .expect_err("规则修改后保存旧预览必须拒绝");
        assert_eq!(err.code, ErrorCode::StalePreview, "返回 STALE_PREVIEW");

        // R2 预览成功后，旧 preview_hash 同样失效
        let (_task2, pv2) = preview_ok(&c, "st12-02-p2", spec2);
        let err = c
            .save_universe(
                "req-st12-02-s3",
                "st12-02-s3",
                SaveUniverseSpec {
                    preview_task_id: task1,
                    preview_hash: pv1.preview_hash,
                    input_hash: pv2.input_hash,
                    name: "篡改池2".to_string(),
                },
            )
            .expect_err("旧 preview_hash 必须拒绝");
        assert_eq!(err.code, ErrorCode::StalePreview);

        // 原已保存池不变
        let after = c.get_universe(&saved.universe_id).expect("读取").expect("存在");
        assert_eq!(after.rule_json, before.rule_json, "原池规则不变");
        assert_eq!(after.members_hash, before.members_hash, "原池成员不变");
        assert_eq!(after.name, "原始池");
    });
}

/// RowsRow → 成员行（S14 起行类型为枚举）。
fn member_of(page: &nautilus_research_domain::protocol::RowsPage) -> &nautilus_research_domain::universe::MemberRow {
    match page.rows.first().expect("至少一行") {
        nautilus_research_domain::protocol::RowsRow::Member(m) => m,
        other => panic!("应为成员行：{other:?}"),
    }
}
