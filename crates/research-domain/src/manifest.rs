//! 快照 manifest：固定 schema、确定性内容（不含 UUID/时间戳），
//! manifest_hash = 规范化 JSON 的 SHA256（架构「数据模型与补齐入口」节）。

use serde::{Deserialize, Serialize};

use crate::{
    hash::hash_canonical,
    protocol::{ImportSource, PriceBasis},
};

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// 分区引用：内容对象相对路径、SHA256、标的、口径与行数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionRef {
    pub relative_path: String,
    pub sha256: String,
    pub instrument_id: String,
    pub price_basis: PriceBasis,
    pub rows: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    pub instruments: u64,
    pub rows: u64,
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub source: ImportSource,
    pub price_basis: PriceBasis,
    pub coverage: Coverage,
    pub partitions: Vec<PartitionRef>,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
}

impl SnapshotManifest {
    /// 确定性内容哈希：manifest 只含规范化研究内容。
    pub fn content_hash(&self) -> String {
        hash_canonical(self)
    }

    /// as_of：覆盖区间的最后完整交易日。
    pub fn as_of(&self) -> &str {
        &self.coverage.end
    }
}

/// 仅行情快照的能力与限制声明（辅助数据能力在后续导入批次扩展）。
pub fn quotes_capabilities() -> Vec<String> {
    vec!["quotes.daily".to_string()]
}

pub fn quotes_limitations() -> Vec<String> {
    vec!["仅日线行情，未导入证券主档、状态、财务、公司行为与交易规则".to_string()]
}
