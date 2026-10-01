//! 任务状态机（架构「任务与提交协议」节）。
//!
//! queued→running→succeeded/failed/cancelled；running→cancelling→cancelled；
//! 异常退出→interrupted；终态不可回退。取消与完成竞争时以协调器已持久化的
//! 完成事务为准；已完成返回 ALREADY_TERMINAL。

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, ResearchError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Queued,
    Running,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Cancelling => "cancelling",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "cancelling" => Ok(Self::Cancelling),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted" => Ok(Self::Interrupted),
            other => Err(ResearchError::invalid(format!("未知任务状态：{other}"))),
        }
    }

    /// 终态不可回退。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// CancelTask 只作用于尚未完成的任务。
    pub fn can_cancel(&self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Cancelling)
    }

    /// 校验一次状态迁移是否合法；非法迁移属于实现缺陷，返回错误而非静默。
    pub fn check_transition(&self, next: TaskState) -> Result<()> {
        let ok = match self {
            Self::Queued => matches!(next, Self::Running | Self::Cancelled | Self::Interrupted),
            Self::Running => matches!(
                next,
                Self::Cancelling | Self::Succeeded | Self::Failed | Self::Interrupted
            ),
            Self::Cancelling => matches!(
                next,
                Self::Cancelled | Self::Succeeded | Self::Failed | Self::Interrupted
            ),
            // 终态不可回退
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(ResearchError::new(
                ErrorCode::AlreadyTerminal,
                format!("非法任务状态迁移：{} → {}", self.as_str(), next.as_str()),
            ))
        }
    }
}
