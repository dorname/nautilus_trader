//! 研究流水线页面状态机（纯逻辑，UT-S15-09 承载）。
//!
//! GUI 不阻塞等待终态：提交命令立即返回 TaskRef，此后每帧轮询
//! `get_task`（轻量只读）推进状态机；有活跃任务时才请求定时重绘
//! （CPU 红线：静默零帧，轮询间隔 500ms）。

use std::time::Duration;

use nautilus_research_domain::protocol::TaskView;
use nautilus_research_domain::task::TaskState;

/// 轮询间隔：有活跃任务时的定时重绘周期。
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 页面任务观察状态机。
#[derive(Debug, Clone)]
pub enum TaskWatch {
    /// 无任务。
    Idle,
    /// 已提交，等待终态（保留提交提示与终态提示）。
    Submitted {
        task_id: String,
        /// 提交提示（如「导入中…」）。
        label: &'static str,
    },
    /// 到达终态（快照保留，页面展示结果或错误）。
    Terminal(TaskView),
}

impl TaskWatch {
    /// 记录提交。
    pub fn submitted(task_id: impl Into<String>, label: &'static str) -> Self {
        TaskWatch::Submitted {
            task_id: task_id.into(),
            label,
        }
    }

    /// 是否有活跃任务（决定是否定时重绘）。
    pub fn is_active(&self) -> bool {
        matches!(self, TaskWatch::Submitted { .. })
    }

    /// 以最新任务视图推进状态机；返回视图是否有变化（驱动 UI 刷新）。
    /// 终态落定后不再推进（保留快照直到下一次提交）。
    pub fn poll(&mut self, view: TaskView) -> bool {
        let TaskWatch::Submitted { task_id, .. } = self else {
            return false;
        };
        if view.task_id != *task_id {
            return false;
        }
        if view.state.is_terminal() {
            *self = TaskWatch::Terminal(view);
        }
        true
    }

    /// 当前任务 ID（活跃或终态）。
    pub fn task_id(&self) -> Option<&str> {
        match self {
            TaskWatch::Submitted { task_id, .. } => Some(task_id),
            TaskWatch::Terminal(v) => Some(&v.task_id),
            TaskWatch::Idle => None,
        }
    }

    /// 终态视图（仅终态有）。
    pub fn terminal(&self) -> Option<&TaskView> {
        match self {
            TaskWatch::Terminal(v) => Some(v),
            _ => None,
        }
    }
}

/// 页面级错误行（提交失败或终态失败的中立呈现）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageError {
    pub text: String,
}

impl PageError {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// 页面状态：表单 + 任务观察 + 最近错误（五页共用骨架）。
#[derive(Debug, Clone)]
pub struct PageState<W> {
    /// 任务观察。
    pub watch: W,
    /// 最近错误（提交失败/终态失败）；成功提交时清空。
    pub error: Option<PageError>,
}

impl Default for PageState<TaskWatch> {
    fn default() -> Self {
        Self {
            watch: TaskWatch::Idle,
            error: None,
        }
    }
}

impl<W> PageState<W> {
    /// 终态失败转为页面错误（终态视图保留，错误行同时提示）。
    pub fn note_terminal_error(&mut self, text: impl Into<String>) {
        self.error = Some(PageError::new(text));
    }
}

impl PageState<TaskWatch> {
    /// 记录提交成功：清错误、进入 Submitted。
    pub fn on_submitted(&mut self, task_id: impl Into<String>, label: &'static str) {
        self.error = None;
        self.watch = TaskWatch::submitted(task_id, label);
    }

    /// 记录提交失败：保留错误，观察状态复位为 Idle（可立即重试）。
    pub fn on_submit_failed(&mut self, text: impl Into<String>) {
        self.error = Some(PageError::new(text));
        self.watch = TaskWatch::Idle;
    }
}

/// 任务视图是否成功。
pub fn is_success(view: &TaskView) -> bool {
    view.state == TaskState::Succeeded
}

/// 协调器拒绝/失败的统一错误行（错误码契约原文 SCREAMING + 中文消息）。
pub fn rejection_text(e: &nautilus_research_domain::ResearchError) -> String {
    let code = serde_json::to_value(&e.code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{:?}", e.code));
    format!("（{code}）：{}", e.message)
}

/// 终态错误的中文行（错误体透传）。
pub fn terminal_error_text(view: &TaskView) -> Option<String> {
    view.error.as_ref().map(|e| {
        format!("任务失败（{}）：{}", e.code, e.message)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(task_id: &str, state: TaskState) -> TaskView {
        TaskView {
            task_id: task_id.into(),
            state,
            last_seq: 1,
            progress: None,
            artifact_hash: None,
            snapshot_id: None,
            error: None,
            children: Vec::new(),
            parent_id: None,
        }
    }

    /// UT-S15-09（断言之组一）：提交→轮询→终态落定的状态机迁移。
    #[test]
    fn watch_submit_poll_terminal() {
        let mut w = TaskWatch::Idle;
        assert!(!w.is_active());
        assert!(w.task_id().is_none());

        w = TaskWatch::submitted("T1", "导入中…");
        assert!(w.is_active());
        assert_eq!(w.task_id(), Some("T1"));

        // 运行中推进：有变化、仍活跃
        let mut changed = w.poll(view("T1", TaskState::Running));
        assert!(changed);
        assert!(w.is_active());

        // 其他任务的视图不推进本页状态
        changed = w.poll(view("OTHER", TaskState::Succeeded));
        assert!(!changed);
        assert!(w.is_active());

        // 终态落定：不再活跃，保留快照
        let mut done = view("T1", TaskState::Succeeded);
        done.artifact_hash = Some("h".into());
        changed = w.poll(done);
        assert!(changed);
        assert!(!w.is_active());
        assert_eq!(w.terminal().map(|v| v.state), Some(TaskState::Succeeded));

        // 终态后轮询不推进（保留快照）
        assert!(!w.poll(view("T1", TaskState::Failed)));
        assert_eq!(w.terminal().map(|v| v.state), Some(TaskState::Succeeded));
    }

    /// UT-S15-09（断言之组二）：PageState 提交成功/失败/终态失败的错误语义。
    #[test]
    fn page_state_error_semantics() {
        let mut ps: PageState<TaskWatch> = PageState::default();
        // 提交成功：清错误、进入 Submitted
        ps.on_submitted("T2", "预览中…");
        assert!(ps.error.is_none());
        assert!(ps.watch.is_active());

        // 终态失败：观察落 Terminal，错误行提示
        let mut failed = view("T2", TaskState::Failed);
        failed.error = Some(nautilus_research_domain::protocol::ErrorBody {
            code: "NOT_FOUND".into(),
            message: "快照不存在：SNAP-404".into(),
            field: None,
            retryable: false,
        });
        ps.watch.poll(failed.clone());
        assert!(!ps.watch.is_active());
        assert_eq!(
            terminal_error_text(&failed).as_deref(),
            Some("任务失败（NOT_FOUND）：快照不存在：SNAP-404")
        );
        ps.note_terminal_error(terminal_error_text(&failed).unwrap_or_default());
        assert!(ps.error.is_some());

        // 下一次提交失败：复位 Idle、错误保留
        let mut ps2: PageState<TaskWatch> = PageState::default();
        ps2.on_submit_failed("缺少必填参数");
        assert!(!ps2.watch.is_active());
        assert!(matches!(ps2.watch, TaskWatch::Idle));
        assert_eq!(ps2.error.as_ref().map(|e| e.text.as_str()), Some("缺少必填参数"));

        // is_success 判定
        assert!(is_success(&view("T", TaskState::Succeeded)));
        assert!(!is_success(&view("T", TaskState::Failed)));
    }
}
