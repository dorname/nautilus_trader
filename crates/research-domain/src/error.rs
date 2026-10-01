//! 契约错误码与统一错误类型（`core-research-contracts.yaml` errors 节）。

use serde::Serialize;

/// 契约定义的错误码，序列化为契约原文的 SCREAMING 名称。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ErrorCode {
    #[serde(rename = "INVALID_ARGUMENT")]
    InvalidArgument,
    #[serde(rename = "NOT_FOUND")]
    NotFound,
    #[serde(rename = "STALE_PREVIEW")]
    StalePreview,
    #[serde(rename = "MISSING_CAPABILITY")]
    MissingCapability,
    #[serde(rename = "EMPTY_UNIVERSE")]
    EmptyUniverse,
    #[serde(rename = "IDEMPOTENCY_CONFLICT")]
    IdempotencyConflict,
    #[serde(rename = "ALREADY_TERMINAL")]
    AlreadyTerminal,
    #[serde(rename = "RUN_NOT_READY")]
    RunNotReady,
    #[serde(rename = "CORRUPT_ARTIFACT")]
    CorruptArtifact,
    #[serde(rename = "NO_OVERLAP")]
    NoOverlap,
    #[serde(rename = "STALE_DATA")]
    StaleData,
    #[serde(rename = "PATH_CONFLICT")]
    PathConflict,
    #[serde(rename = "BUSY")]
    Busy,
    #[serde(rename = "DISK_FULL")]
    DiskFull,
    #[serde(rename = "WORKER_EXITED")]
    WorkerExited,
    #[serde(rename = "PROTOCOL_VERSION")]
    ProtocolVersion,
}

/// 统一错误：code/message 必备，field 为字段路径（中文原因写入 message）。
#[derive(Debug, Clone, Serialize)]
pub struct ResearchError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub retryable: bool,
}

impl ResearchError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let retryable = matches!(code, ErrorCode::Busy);
        Self {
            code,
            message: message.into(),
            field: None,
            retryable,
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.field = Some(field.into());
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArgument, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Busy, message)
    }
}

impl std::fmt::Display for ResearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for ResearchError {}

pub type Result<T> = std::result::Result<T, ResearchError>;
