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

/// 辅助数据引用（manifest.auxiliary）：种类、内容对象与行数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuxRef {
    pub kind: String,
    pub relative_path: String,
    pub sha256: String,
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
    /// 辅助数据内容对象（master/financial 等），按 kind 排序。
    #[serde(default)]
    pub auxiliary: Vec<AuxRef>,
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

    /// 指定种类的辅助数据引用。
    pub fn aux(&self, kind: &str) -> Option<&AuxRef> {
        self.auxiliary.iter().find(|a| a.kind == kind)
    }
}

/// 按实际内容生成能力与限制声明。
pub fn capabilities(has_quotes: bool, aux_kinds: &[&str]) -> Vec<String> {
    let mut caps = Vec::new();
    if has_quotes {
        caps.push("quotes.daily".to_string());
    }
    for k in aux_kinds {
        caps.push(format!("aux.{k}"));
    }
    caps.sort();
    caps
}

pub fn limitations(aux_kinds: &[&str]) -> Vec<String> {
    let mut missing = Vec::new();
    for (kind, label) in [
        ("master", "证券主档"),
        ("financial", "财务报告"),
        ("status", "状态表"),
        ("actions", "公司行为"),
        ("rules", "交易规则"),
    ] {
        if !aux_kinds.contains(&kind) {
            missing.push(label);
        }
    }
    if missing.is_empty() {
        vec![]
    } else {
        vec![format!("未导入：{}；涉及这些数据的条件评估为未知", missing.join("、"))]
    }
}
