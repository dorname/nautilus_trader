//! 结果输出：`--json` 机器可读与中文 key-value 表格双形态（字段同源）。

use std::io::Write;

use nautilus_research_domain::protocol::{Comparison, SnapshotPage, TaskRef, TaskView};
use nautilus_research_domain::protocol::{UniverseRef, UniversePreview};
use nautilus_research_domain::plan::{ExportReceipt, NoteRef};
use nautilus_research_domain::task::TaskState;

/// JSON 输出（pretty；序列化失败视为程序错误，写中文诊断）。
pub fn emit_json<T: serde::Serialize>(w: &mut impl Write, value: &T) {
    let _ = match serde_json::to_string_pretty(value) {
        Ok(s) => writeln!(w, "{s}"),
        Err(e) => writeln!(w, "结果序列化失败：{e}"),
    };
}

/// key-value 表格输出。
pub fn emit_kv(w: &mut impl Write, rows: &[(&str, String)]) {
    for (k, v) in rows {
        let _ = writeln!(w, "{k}: {v}");
    }
    let _ = writeln!(w);
}

fn state_cn(state: &TaskState) -> String {
    match state {
        TaskState::Queued => "排队中".to_string(),
        TaskState::Running => "运行中".to_string(),
        TaskState::Cancelling => "取消中".to_string(),
        TaskState::Succeeded => "成功".to_string(),
        TaskState::Failed => "失败".to_string(),
        TaskState::Cancelled => "已取消".to_string(),
        TaskState::Interrupted => "中断（重启扫描）".to_string(),
    }
}

/// TaskRef 视图行。
pub fn task_ref_rows(r: &TaskRef) -> Vec<(&'static str, String)> {
    vec![
        ("任务 ID", r.task_id.clone()),
        ("请求 ID", r.request_id.clone()),
        ("状态", state_cn(&r.state)),
        ("提示", "run show --task <ID> 查询进度；run cancel 取消".to_string()),
    ]
}

/// TaskView 视图行（进度/产物/错误/网格子任务摘要）。
pub fn task_view_rows(v: &TaskView) -> Vec<(&'static str, String)> {
    let mut rows: Vec<(&'static str, String)> = vec![
        ("任务 ID", v.task_id.clone()),
        ("状态", state_cn(&v.state)),
    ];
    if let Some(p) = &v.progress {
        let total = p.total.map(|t| format!("/{t}")).unwrap_or_default();
        let done = p.done.map(|d| d.to_string()).unwrap_or_default();
        rows.push(("进度", format!("{} {}{}", p.stage, done, total)));
    }
    if let Some(s) = &v.snapshot_id {
        rows.push(("快照 ID", s.clone()));
    }
    if let Some(h) = &v.artifact_hash {
        rows.push(("产物哈希", h.clone()));
    }
    if let Some(e) = &v.error {
        let field = e.field.as_deref().unwrap_or("-");
        rows.push(("错误", format!("{}（字段 {field}）：{}", e.code, e.message)));
    }
    if !v.children.is_empty() {
        rows.push(("网格子任务", v.children.len().to_string()));
        for c in &v.children {
            rows.push(("  子任务", format!("{} [{}] 结果 {}", c.task_id, state_cn(&c.state), c.result_hash.clone().unwrap_or_else(|| "无".into()))));
        }
    }
    if !matches!(v.state, TaskState::Succeeded | TaskState::Failed | TaskState::Cancelled | TaskState::Interrupted) {
        rows.push(("提示", "task wait --task <ID> --timeout <秒> 等待终态".to_string()));
    }
    rows
}

/// SnapshotPage 视图行。
pub fn snapshots_rows(page: &SnapshotPage) -> Vec<(&'static str, String)> {
    let mut rows: Vec<(&'static str, String)> = vec![("快照数", page.items.len().to_string())];
    for s in &page.items {
        rows.push(("快照", format!("{} as_of={} 清单 {}", s.snapshot_id, s.as_of, &s.manifest_hash[..12.min(s.manifest_hash.len())])));
    }
    if let Some(c) = &page.next_cursor {
        rows.push(("下一页游标", c.clone()));
    }
    rows
}

/// UniversePreview 视图行。
pub fn preview_rows(p: &UniversePreview) -> Vec<(&'static str, String)> {
    vec![
        ("预览哈希", p.preview_hash.clone()),
        ("输入哈希", p.input_hash.clone()),
        ("通过", p.pass.to_string()),
        ("排除", p.exclude.to_string()),
        ("未知", p.unknown.to_string()),
        ("快照", p.snapshot_id.clone()),
        ("as_of", p.as_of.clone()),
        ("提示", "universe save --preview-hash <上值> --input-hash <上值> 保存".to_string()),
    ]
}

/// UniverseRef 视图行。
pub fn universe_rows(u: &UniverseRef) -> Vec<(&'static str, String)> {
    vec![
        ("股票池 ID", u.universe_id.clone()),
        ("版本哈希", u.version_hash.clone()),
        ("规则哈希", u.rule_hash.clone()),
        ("快照", u.snapshot_id.clone()),
        ("as_of", u.as_of.clone()),
        ("成员数", u.count.to_string()),
    ]
}

/// Comparison 摘要视图行。
pub fn comparison_rows(c: &Comparison) -> Vec<(&'static str, String)> {
    let mut rows: Vec<(&'static str, String)> = Vec::new();
    for r in &c.runs {
        let m = &r.metrics;
        rows.push((
            "运行",
            format!(
                "{} {}~{} 总收益 {} 夏普 {} 回撤 {}",
                &r.run_id[..12.min(r.run_id.len())],
                r.start,
                r.end,
                fmt_metric(&m.total_return),
                fmt_metric(&m.sharpe),
                fmt_metric(&m.max_drawdown),
            ),
        ));
    }
    if let Some(o) = &c.overlap {
        rows.push(("交集区间", format!("{} ~ {}", o.start, o.end)));
    }
    if let Some(b) = &c.benchmark {
        rows.push(("基准（等权买入持有）", fmt_metric(&b.metrics.total_return)));
    }
    for d in &c.differences {
        rows.push(("差异", format!("[{}] {}", d.kind, d.detail)));
    }
    rows.push(("提示", "--json 获取完整指标（含年化/波动/换手/成本）".to_string()));
    rows
}

fn fmt_metric(m: &nautilus_research_domain::metrics::NullableMetric) -> String {
    m.value.clone().unwrap_or_else(|| "—".to_string())
}

/// ExportReceipt 视图行。
pub fn export_rows(r: &ExportReceipt) -> Vec<(&'static str, String)> {
    vec![
        ("导出 ID", r.export_id.clone()),
        ("路径", r.path.clone()),
        ("SHA256", r.sha256.clone()),
        ("行数", r.rows.to_string()),
    ]
}

/// NoteRef 视图行。
pub fn note_rows(n: &NoteRef) -> Vec<(&'static str, String)> {
    vec![("备注 ID", n.note_id.clone()), ("计划 ID", n.plan_id.clone())]
}
