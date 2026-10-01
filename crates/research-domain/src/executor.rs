//! 研究运行执行器挂载点：进程内路径由引擎侧（research-worker）注册，
//! 领域核心不反向依赖引擎 crate。
//!
//! 退出码契约（进程内与子进程一致）：
//! - `0` 正常完成（产物已写入对象存储，终态由协调线程的持久化裁决确定）；
//! - `EXIT_CANCELLED` 取消（提交前发现取消标记，无产物提交）；
//! - `EXIT_CRASHED` 提交前崩溃（测试注入，无产物提交）；
//! - 其他非零 失败（配置/数据/引擎错误）。

use std::{path::Path, sync::atomic::AtomicBool};

use crate::coordinator::Completion;

pub(crate) type RunExecutor = dyn Fn(&Path, &str, &dyn Fn() -> bool, &str, &crossbeam::channel::Sender<Completion>) -> i32
    + Send
    + Sync;

pub const EXIT_OK: i32 = 0;
pub const EXIT_CANCELLED: i32 = 3;
pub const EXIT_CRASHED: i32 = 4;

static RUN_EXECUTOR: std::sync::RwLock<Option<std::sync::Arc<RunExecutor>>> =
    std::sync::RwLock::new(None);

/// 注册进程内执行器（引擎侧启动时调用一次；重复注册覆盖）。
pub fn register_run_executor(f: std::sync::Arc<RunExecutor>) {
    if let Ok(mut slot) = RUN_EXECUTOR.write() {
        *slot = Some(f);
    }
}

/// 执行一次运行（有注册执行器走进程内；未注册按 WORKER_EXITED 失败）。
pub(crate) fn run_in_process(
    workspace: &Path,
    config_hash: &str,
    cancel: &AtomicBool,
    task_id: &str,
    done: &crossbeam::channel::Sender<Completion>,
) -> i32 {
    let executor = RUN_EXECUTOR.read().ok().and_then(|g| g.clone());
    match executor {
        Some(f) => {
            let probe = || cancel.load(std::sync::atomic::Ordering::SeqCst);
            f(workspace, config_hash, &probe, task_id, done)
        }
        None => {
            let _ = done.send(Completion::Failed {
                task_id: task_id.to_string(),
                error: crate::error::ResearchError::new(
                    crate::error::ErrorCode::WorkerExited,
                    "未注册研究运行执行器（需引擎侧注册或配置 worker_bin）",
                ),
            });
            1
        }
    }
}
