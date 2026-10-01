//! 研究桌面领域核心：任务生命周期、数据快照导入、内容寻址存储与应用协调器。
//!
//! 规格来源：
//! - 架构：`logos/resources/prd/3-technical-plan/1-architecture/core-05-research-architecture.md`
//! - 契约：`logos/resources/api/core-research-contracts.yaml`
//! - DDL：`logos/resources/database/core-01-research-storage.sql`（经 `include_str!` 单源引用）
//! - 场景：`core-S11-data-snapshot.md`
//!
//! CPU 约束（验收红线：不打爆 CPU）：协调器工作线程通过 `crossbeam::channel`
//! 阻塞接收命令与任务完成通知，空闲时线程挂起，不存在轮询空转；
//! 计算线程仅在任务执行期间存活，按行批次边界响应取消。

pub mod auxiliary;
pub mod coordinator;
pub mod corporate;
pub mod error;
pub mod executor;
pub mod gate;
pub mod hash;
pub mod indicators;
pub mod manifest;
pub mod metrics;
pub mod objects;
pub mod plan;
pub mod parquet_io;
pub mod protocol;
pub mod quotes;
pub mod signals;
pub mod store;
pub mod task;
pub mod time;
pub mod universe;
pub mod worker_api;

pub use coordinator::{Coordinator, CoordinatorConfig, ImportHooks, RunConfigDoc};
pub use executor::register_run_executor;
pub use error::{ErrorCode, ResearchError};
pub use protocol::*;
pub use quotes::QuoteRow;
pub use task::TaskState;
