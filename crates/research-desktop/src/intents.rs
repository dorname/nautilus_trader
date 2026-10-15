//! 意图即 Artifact（Intent-as-Artifact）——声明式注册表、L0/L1/L2 入口与确定性风险门。
//!
//! 本模块只处理识别与归约，不直接执行业务动作；执行由调用方在 `app.rs` 的
//! 确定性风险门后分发。所有模型输出只作为 Artifact 建议，注册表中的
//! `risk_class` 是风险门唯一权威来源。

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

/// 风险等级：注册表唯一权威值，模型回显仅作格式校验。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    /// 查询/分析/草稿类，直接消费。
    LocalAction,
    /// 执行类，须经人类确认。
    NeedsConfirm,
    /// 真实下单等违反产品边界，硬阻断。
    Forbidden,
}

impl std::fmt::Display for RiskClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskClass::LocalAction => write!(f, "local_action"),
            RiskClass::NeedsConfirm => write!(f, "needs_confirm"),
            RiskClass::Forbidden => write!(f, "forbidden"),
        }
    }
}

/// 槽位值类型声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotType {
    String,
    Int,
    Number,
    Bool,
}

/// 槽位必要性声明。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotRequired {
    Required,
    Optional,
}

/// 单个槽位声明，如 `version = "optional<int>"`。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SlotDecl {
    pub required: SlotRequired,
    pub ty: SlotType,
}

impl std::fmt::Display for SlotDecl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let req = match self.required {
            SlotRequired::Required => "required",
            SlotRequired::Optional => "optional",
        };
        let ty = match self.ty {
            SlotType::String => "string",
            SlotType::Int => "int",
            SlotType::Number => "number",
            SlotType::Bool => "bool",
        };
        write!(f, "{req}<{ty}>")
    }
}

impl std::str::FromStr for SlotDecl {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let (req_part, ty_part) = if let Some((a, b)) = s.split_once('<') {
            (a.trim(), b.trim().strip_suffix('>').unwrap_or(b).trim())
        } else {
            return Err(format!("槽位声明缺少类型包裹：{s}"));
        };
        let required = match req_part {
            "required" => SlotRequired::Required,
            "optional" => SlotRequired::Optional,
            other => return Err(format!("未知必要性修饰符：{other}")),
        };
        let ty = match ty_part {
            "string" => SlotType::String,
            "int" => SlotType::Int,
            "number" => SlotType::Number,
            "bool" => SlotType::Bool,
            other => return Err(format!("未知槽位类型：{other}")),
        };
        Ok(SlotDecl { required, ty })
    }
}

/// 注册表原始 TOML 结构。
#[derive(Debug, Clone, Deserialize)]
pub struct IntentTypeToml {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub boundary_negative: Vec<String>,
    #[serde(default)]
    pub slots: BTreeMap<String, String>,
    pub risk_class: RiskClass,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntentTypeFile {
    pub r#type: IntentTypeToml,
}

/// 解析后的意图类型。
#[derive(Debug, Clone, PartialEq)]
pub struct IntentType {
    pub id: String,
    pub description: String,
    pub boundary_negative: Vec<String>,
    pub slots: BTreeMap<String, SlotDecl>,
    pub risk_class: RiskClass,
    pub examples: Vec<String>,
}

/// 结构化 Artifact，由 L0 或 L1 归约产生。
#[derive(Debug, Clone, PartialEq)]
pub struct IntentArtifact {
    pub type_id: String,
    pub slots: BTreeMap<String, serde_json::Value>,
    pub confidence: f64,
}

/// 业务 Intent 枚举（14 个业务类型 + Unknown 兜底）。
/// 注册表可执行类型必须存在唯一映射到此枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intent {
    ConfirmRequirement,
    DraftRequirement,
    NewProject,
    SwitchProject,
    GenerateDesign,
    GenerateCode,
    SaveVersion,
    RunExperiment,
    CompareExperiments,
    MakeReport,
    ExplainBoundary,
    PlanGenerate,
    PlanCheck,
    PlanExport,
    Unknown,
}

impl Intent {
    /// 现有 14 个业务意图的注册表类型 ID。
    pub fn type_id(self) -> &'static str {
        match self {
            Intent::ConfirmRequirement => "requirement.confirm.requested",
            Intent::DraftRequirement => "requirement.draft.requested",
            Intent::NewProject => "project.create.requested",
            Intent::SwitchProject => "project.switch.requested",
            Intent::GenerateDesign => "design.generate.requested",
            Intent::GenerateCode => "code.generate.requested",
            Intent::SaveVersion => "version.save.requested",
            Intent::RunExperiment => "experiment.run.requested",
            Intent::CompareExperiments => "experiment.compare.requested",
            Intent::MakeReport => "report.generate.requested",
            Intent::ExplainBoundary => "boundary.explain.requested",
            Intent::PlanGenerate => "plan.generate.requested",
            Intent::PlanCheck => "plan.check.requested",
            Intent::PlanExport => "plan.export.requested",
            Intent::Unknown => "unknown",
        }
    }

    /// 由注册表类型 ID 映射到业务 Intent；forbidden 与未知 ID 返回 None。
    pub fn from_type_id(type_id: &str) -> Option<Intent> {
        match type_id {
            "requirement.confirm.requested" => Some(Intent::ConfirmRequirement),
            "requirement.draft.requested" => Some(Intent::DraftRequirement),
            "project.create.requested" => Some(Intent::NewProject),
            "project.switch.requested" => Some(Intent::SwitchProject),
            "design.generate.requested" => Some(Intent::GenerateDesign),
            "code.generate.requested" => Some(Intent::GenerateCode),
            "version.save.requested" => Some(Intent::SaveVersion),
            "experiment.run.requested" => Some(Intent::RunExperiment),
            "experiment.compare.requested" => Some(Intent::CompareExperiments),
            "report.generate.requested" => Some(Intent::MakeReport),
            "boundary.explain.requested" => Some(Intent::ExplainBoundary),
            "plan.generate.requested" => Some(Intent::PlanGenerate),
            "plan.check.requested" => Some(Intent::PlanCheck),
            "plan.export.requested" => Some(Intent::PlanExport),
            _ => None,
        }
    }
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.type_id())
    }
}

/// 注册表加载与校验错误。
#[derive(Debug, Clone, PartialEq)]
pub enum RegistryError {
    DuplicateId(String),
    MissingField(String, &'static str),
    InvalidSlotDecl(String, String),
    UnknownRiskClass(String),
    NoIntentMapping(String),
    ForbiddenHasMapping(String),
    InvalidId(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::DuplicateId(id) => write!(f, "重复意图 ID：{id}"),
            RegistryError::MissingField(id, field) => {
                write!(f, "意图 {id} 缺少必填字段：{field}")
            }
            RegistryError::InvalidSlotDecl(id, e) => {
                write!(f, "意图 {id} 槽位声明非法：{e}")
            }
            RegistryError::UnknownRiskClass(id) => {
                write!(f, "意图 {id} 的 risk_class 不在允许枚举内")
            }
            RegistryError::NoIntentMapping(id) => {
                write!(f, "可执行意图 {id} 缺少到业务 Intent 的唯一映射")
            }
            RegistryError::ForbiddenHasMapping(id) => {
                write!(f, "forbidden 意图 {id} 不得存在业务 Intent 映射")
            }
            RegistryError::InvalidId(id) => write!(f, "意图 ID 非法或为空：{id}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// 意图注册表，编译期内嵌。
#[derive(Debug, Clone, PartialEq)]
pub struct IntentRegistry {
    types: HashMap<String, IntentType>,
}

impl IntentRegistry {
    /// 从原始 manifest 列表构建注册表；失败返回第一个错误。
    pub fn from_manifests(manifests: Vec<IntentTypeToml>) -> Result<Self, RegistryError> {
        let mut types = HashMap::with_capacity(manifests.len());
        for raw in manifests {
            if raw.id.trim().is_empty() {
                return Err(RegistryError::InvalidId(raw.id));
            }
            if raw.description.trim().is_empty() {
                return Err(RegistryError::MissingField(raw.id.clone(), "description"));
            }
            // risk_class 已由 serde 强类型校验，但保留显式分支以提供清晰错误
            let _ = raw.risk_class;

            let mut slots = BTreeMap::new();
            for (name, decl_str) in &raw.slots {
                let decl = decl_str.parse::<SlotDecl>().map_err(|e| {
                    RegistryError::InvalidSlotDecl(raw.id.clone(), format!("{name}: {e}"))
                })?;
                slots.insert(name.clone(), decl);
            }

            let ty = IntentType {
                id: raw.id.clone(),
                description: raw.description,
                boundary_negative: raw.boundary_negative,
                slots,
                risk_class: raw.risk_class,
                examples: raw.examples,
            };
            if types.insert(raw.id.clone(), ty).is_some() {
                return Err(RegistryError::DuplicateId(raw.id));
            }
        }

        // 二次校验：映射约束
        for ty in types.values() {
            match ty.risk_class {
                RiskClass::Forbidden => {
                    if Intent::from_type_id(&ty.id).is_some() {
                        return Err(RegistryError::ForbiddenHasMapping(ty.id.clone()));
                    }
                }
                _ => {
                    if Intent::from_type_id(&ty.id).is_none() {
                        return Err(RegistryError::NoIntentMapping(ty.id.clone()));
                    }
                }
            }
        }

        Ok(Self { types })
    }

    /// 加载内嵌注册表（`logos/intents/*.toml`）。
    pub fn embedded() -> Result<Self, RegistryError> {
        const MANIFESTS: &[&str] = &[
            include_str!("../../../logos/intents/requirement.confirm.requested.toml"),
            include_str!("../../../logos/intents/requirement.draft.requested.toml"),
            include_str!("../../../logos/intents/project.create.requested.toml"),
            include_str!("../../../logos/intents/project.switch.requested.toml"),
            include_str!("../../../logos/intents/design.generate.requested.toml"),
            include_str!("../../../logos/intents/code.generate.requested.toml"),
            include_str!("../../../logos/intents/version.save.requested.toml"),
            include_str!("../../../logos/intents/experiment.run.requested.toml"),
            include_str!("../../../logos/intents/experiment.compare.requested.toml"),
            include_str!("../../../logos/intents/report.generate.requested.toml"),
            include_str!("../../../logos/intents/boundary.explain.requested.toml"),
            include_str!("../../../logos/intents/plan.generate.requested.toml"),
            include_str!("../../../logos/intents/plan.check.requested.toml"),
            include_str!("../../../logos/intents/plan.export.requested.toml"),
            include_str!("../../../logos/intents/order.live.submit.requested.toml"),
        ];
        let mut parsed = Vec::with_capacity(MANIFESTS.len());
        for text in MANIFESTS {
            let file: IntentTypeFile = toml::from_str(text)
                .map_err(|e| RegistryError::InvalidId(format!("TOML 解析失败：{e}")))?;
            parsed.push(file.r#type);
        }
        Self::from_manifests(parsed)
    }

    pub fn get(&self, type_id: &str) -> Option<&IntentType> {
        self.types.get(type_id)
    }

    pub fn types(&self) -> &HashMap<String, IntentType> {
        &self.types
    }

    pub fn risk_class(&self, type_id: &str) -> Option<RiskClass> {
        self.types.get(type_id).map(|t| t.risk_class)
    }
}

impl Default for IntentRegistry {
    fn default() -> Self {
        Self::embedded().expect("内嵌意图注册表必须合法")
    }
}

/// Triage 候选模型输出。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TriageCandidate {
    pub type_id: String,
    #[serde(default)]
    pub slots: BTreeMap<String, serde_json::Value>,
    pub confidence: f64,
    #[serde(default)]
    pub risk_class: Option<RiskClass>,
}

/// Triage 顶层模型输出。
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct TriageOutput {
    pub domain: String,
    #[serde(default)]
    pub candidates: Vec<TriageCandidate>,
}

/// Triage 归约结果。
#[derive(Debug, Clone, PartialEq)]
pub enum TriageResult {
    /// 应进入 ChatFree 自由问答。
    ChatFree,
    /// 构造出合法 Artifact；风险由注册表权威决定。
    Artifact(IntentArtifact),
    /// 需要澄清。
    ClarificationRequested(Vec<String>),
    /// 结构化输出非法，调用方可回灌重试。
    Invalid(String),
}

/// 从模型回复文本提取最外层 JSON 对象（容忍 ```json fence）。
pub fn extract_json_object(text: &str) -> Option<&str> {
    let text = text.trim();
    // 若被 fence 包裹，先去掉 fence
    let body = if text.starts_with("```") {
        let inner = text
            .strip_prefix("```json")
            .or_else(|| text.strip_prefix("```"))?;
        let inner = inner.trim();
        inner.strip_suffix("```").unwrap_or(inner).trim()
    } else {
        text
    };
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    (start <= end).then(|| &body[start..=end])
}

/// 校验候选是否符合注册表与槽位契约。
pub fn validate_candidate(
    registry: &IntentRegistry,
    candidate: &TriageCandidate,
) -> Result<(), String> {
    let ty = registry
        .get(&candidate.type_id)
        .ok_or_else(|| format!("未知 type_id：{}", candidate.type_id))?;
    if candidate.confidence < 0.0 || candidate.confidence > 1.0 {
        return Err(format!("confidence 越界：{}", candidate.confidence));
    }
    // 检查槽位：必填不缺、无未声明、类型匹配
    for (name, decl) in &ty.slots {
        match decl.required {
            SlotRequired::Required => {
                candidate
                    .slots
                    .get(name)
                    .ok_or_else(|| format!("缺少必填槽位 {name}"))?;
            }
            SlotRequired::Optional => {}
        }
    }
    for (name, value) in &candidate.slots {
        let decl = ty
            .slots
            .get(name)
            .ok_or_else(|| format!("未声明槽位 {name}"))?;
        let ok = match decl.ty {
            SlotType::String => value.is_string(),
            SlotType::Int => value.as_i64().is_some(),
            SlotType::Number => value.as_f64().is_some() || value.as_i64().is_some(),
            SlotType::Bool => value.is_boolean(),
        };
        if !ok {
            return Err(format!("槽位 {name} 类型不匹配"));
        }
    }
    Ok(())
}

/// 归约一次 Triage 输出。返回 Invalid 时可由调用方回灌重试。
pub fn reduce_triage_output(
    registry: &IntentRegistry,
    output: &TriageOutput,
    min_confidence: f64,
    min_gap: f64,
    fallback_candidates: &[String],
) -> TriageResult {
    if output.domain == "out_of_scope" {
        if output.candidates.is_empty() {
            return TriageResult::ChatFree;
        }
        return TriageResult::Invalid("域外输出不得携带候选".into());
    }
    if output.domain != "in_scope" {
        return TriageResult::Invalid(format!("未知 domain：{}", output.domain));
    }

    // 去重、按置信度排序
    let mut seen = std::collections::HashSet::new();
    let mut valid: Vec<&TriageCandidate> = output
        .candidates
        .iter()
        .filter(|c| {
            if !seen.insert(&c.type_id) {
                return false;
            }
            validate_candidate(registry, c).is_ok()
        })
        .collect();
    valid.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    if valid.is_empty() {
        return TriageResult::ClarificationRequested(fallback_candidates.to_vec());
    }

    let top1 = valid[0];
    let top2 = valid.get(1).map(|c| c.confidence).unwrap_or(0.0);

    // 特殊硬阻断：只要模型明确给出 forbidden 候选且置信达标，
    // 不再因与其他候选差距过近而进入澄清，直接返回该 Artifact 由风险门阻断。
    if registry.risk_class(&top1.type_id) == Some(RiskClass::Forbidden)
        && top1.confidence >= min_confidence
    {
        return TriageResult::Artifact(IntentArtifact {
            type_id: top1.type_id.clone(),
            slots: top1.slots.clone(),
            confidence: top1.confidence,
        });
    }

    if top1.confidence < min_confidence || (top1.confidence - top2) < min_gap {
        let candidates: Vec<String> = valid
            .iter()
            .take(3)
            .map(|c| c.type_id.clone())
            .collect();
        return TriageResult::ClarificationRequested(candidates);
    }

    TriageResult::Artifact(IntentArtifact {
        type_id: top1.type_id.clone(),
        slots: top1.slots.clone(),
        confidence: top1.confidence,
    })
}

/// 解析模型回复文本并归约；失败返回 Invalid。
pub fn parse_and_reduce_triage_reply(
    registry: &IntentRegistry,
    reply: &str,
    min_confidence: f64,
    min_gap: f64,
    fallback_candidates: &[String],
) -> TriageResult {
    let body = match extract_json_object(reply) {
        Some(b) => b,
        None => return TriageResult::Invalid("回复中未找到 JSON 对象".into()),
    };
    let output: TriageOutput = match serde_json::from_str(body) {
        Ok(o) => o,
        Err(e) => return TriageResult::Invalid(format!("JSON 解析失败：{e}")),
    };
    reduce_triage_output(registry, &output, min_confidence, min_gap, fallback_candidates)
}

/// 风险门输出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskGateResult {
    /// 直接分发。
    Allow,
    /// 需要人类确认。
    NeedsConfirmation,
    /// 硬阻断。
    Forbidden,
}

/// 确定性风险门：以注册表权威风险等级为准，确认状态仅放行 needs_confirm 一次。
pub fn apply_risk_gate(
    registry: &IntentRegistry,
    artifact: &IntentArtifact,
    confirmed: bool,
) -> RiskGateResult {
    match registry.risk_class(&artifact.type_id) {
        Some(RiskClass::Forbidden) => RiskGateResult::Forbidden,
        Some(RiskClass::NeedsConfirm) => {
            if confirmed {
                RiskGateResult::Allow
            } else {
                RiskGateResult::NeedsConfirmation
            }
        }
        Some(RiskClass::LocalAction) => RiskGateResult::Allow,
        None => RiskGateResult::Forbidden,
    }
}

/// L0 显式命令与芯片匹配：只有精确命中才返回对应 Intent，其余返回 Unknown。
/// 注意：本函数**不**解析槽位，L0 只负责触发类型；槽位由后续处理器按需读取。
pub fn route(text: &str) -> Intent {
    let t = text.trim();
    if t.is_empty() {
        return Intent::Unknown;
    }
    // 芯片/精确命令：只认稳定口令，不认宽泛包含。
    match t {
        "确认需求" => Intent::ConfirmRequirement,
        "生成需求草稿" => Intent::DraftRequirement,
        "新建项目" => Intent::NewProject,
        "切换项目" => Intent::SwitchProject,
        "生成设计" => Intent::GenerateDesign,
        "生成代码" => Intent::GenerateCode,
        "保存版本" => Intent::SaveVersion,
        "运行实验" => Intent::RunExperiment,
        "比较实验" => Intent::CompareExperiments,
        "生成报告" | "生成验证报告" => Intent::MakeReport,
        "解释验证区别" => Intent::ExplainBoundary,
        "生成计划" => Intent::PlanGenerate,
        "核对计划" => Intent::PlanCheck,
        "导出 CSV" => Intent::PlanExport,
        _ => Intent::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_loads() {
        nautilus_research_testkit::case("UT-S24-01", || {
            let reg = IntentRegistry::embedded().expect("内嵌注册表应加载成功");
            assert_eq!(reg.types().len(), 15, "14 业务 + 1 forbidden");
            assert!(reg.get("experiment.run.requested").is_some());
            assert_eq!(
                reg.risk_class("experiment.run.requested"),
                Some(RiskClass::NeedsConfirm)
            );
            assert_eq!(
                reg.risk_class("plan.export.requested"),
                Some(RiskClass::NeedsConfirm)
            );
            assert_eq!(
                reg.risk_class("order.live.submit.requested"),
                Some(RiskClass::Forbidden)
            );
            assert!(Intent::from_type_id("order.live.submit.requested").is_none());
        });
    }

    #[test]
    fn registry_rejects_duplicate() {
        let manifests = vec![
            IntentTypeToml {
                id: "a.b".into(),
                description: "x".into(),
                boundary_negative: vec![],
                slots: BTreeMap::new(),
                risk_class: RiskClass::LocalAction,
                examples: vec![],
            },
            IntentTypeToml {
                id: "a.b".into(),
                description: "y".into(),
                boundary_negative: vec![],
                slots: BTreeMap::new(),
                risk_class: RiskClass::LocalAction,
                examples: vec![],
            },
        ];
        assert!(matches!(
            IntentRegistry::from_manifests(manifests).unwrap_err(),
            RegistryError::DuplicateId(id) if id == "a.b"
        ));
    }

    #[test]
    fn l0_exact_only() {
        nautilus_research_testkit::case("UT-S24-02", || {
            assert_eq!(route("确认需求"), Intent::ConfirmRequirement);
            assert_eq!(route("生成需求草稿"), Intent::DraftRequirement);
            assert_eq!(route("运行实验"), Intent::RunExperiment);
            assert_eq!(route("导出 CSV"), Intent::PlanExport);
            // 宽泛或否定输入不再因 contains 命中
            assert_eq!(route("帮我跑一下"), Intent::Unknown);
            assert_eq!(route("帮我看看设计"), Intent::Unknown);
            assert_eq!(route("先别跑实验"), Intent::Unknown);
            assert_eq!(route("不要导出"), Intent::Unknown);
            assert_eq!(route(""), Intent::Unknown);
        });
    }

    #[test]
    fn triage_extracts_json_with_fence() {
        let text = "```json\n{\"domain\":\"out_of_scope\",\"candidates\":[]}\n```";
        assert_eq!(
            extract_json_object(text),
            Some(r#"{"domain":"out_of_scope","candidates":[]}"#)
        );
    }

    #[test]
    fn triage_retry_and_clarification() {
        nautilus_research_testkit::case("UT-S24-04", || {
            let reg = IntentRegistry::embedded().unwrap();
            // 域外 → ChatFree
            let out_of_scope = TriageOutput {
                domain: "out_of_scope".into(),
                candidates: vec![],
            };
            assert_eq!(
                reduce_triage_output(&reg, &out_of_scope, 0.7, 0.15, &[]),
                TriageResult::ChatFree
            );
            // 低置信 → ClarificationRequested
            let low_conf = TriageOutput {
                domain: "in_scope".into(),
                candidates: vec![TriageCandidate {
                    type_id: "experiment.run.requested".into(),
                    slots: BTreeMap::new(),
                    confidence: 0.6,
                    risk_class: None,
                }],
            };
            assert!(matches!(
                reduce_triage_output(&reg, &low_conf, 0.7, 0.15, &[]),
                TriageResult::ClarificationRequested(_)
            ));
            // top1/top2 过近 → ClarificationRequested
            let close = TriageOutput {
                domain: "in_scope".into(),
                candidates: vec![
                    TriageCandidate {
                        type_id: "design.generate.requested".into(),
                        slots: BTreeMap::new(),
                        confidence: 0.72,
                        risk_class: None,
                    },
                    TriageCandidate {
                        type_id: "boundary.explain.requested".into(),
                        slots: BTreeMap::new(),
                        confidence: 0.65,
                        risk_class: None,
                    },
                ],
            };
            assert!(matches!(
                reduce_triage_output(&reg, &close, 0.7, 0.15, &[]),
                TriageResult::ClarificationRequested(_)
            ));
            // 首次非法 → 调用方重试（reduce 本身返回 Invalid）
            let bad = TriageOutput {
                domain: "unknown_domain".into(),
                candidates: vec![TriageCandidate {
                    type_id: "experiment.run.requested".into(),
                    slots: BTreeMap::new(),
                    confidence: 0.95,
                    risk_class: None,
                }],
            };
            assert!(
                matches!(
                    reduce_triage_output(&reg, &bad, 0.7, 0.15, &[]),
                    TriageResult::Invalid(_)
                ),
                "未知 domain 应返回 Invalid 供调用方重试"
            );
        });
    }

    #[test]
    fn triage_artifact_uses_registry_risk_class() {
        nautilus_research_testkit::case("UT-S24-03", || {
            let reg = IntentRegistry::embedded().unwrap();
            let out = TriageOutput {
                domain: "in_scope".into(),
                candidates: vec![TriageCandidate {
                    type_id: "experiment.run.requested".into(),
                    slots: BTreeMap::new(),
                    confidence: 0.95,
                    risk_class: Some(RiskClass::LocalAction), // 模型伪造 local_action
                }],
            };
            let result = reduce_triage_output(&reg, &out, 0.7, 0.15, &[]);
            assert!(matches!(result, TriageResult::Artifact(_)));
            if let TriageResult::Artifact(a) = result {
                assert_eq!(a.type_id, "experiment.run.requested");
                assert_eq!(reg.risk_class(&a.type_id), Some(RiskClass::NeedsConfirm));
            }
        });
    }

    #[test]
    fn risk_gate_block_injection() {
        nautilus_research_testkit::case("UT-S24-05", || {
            let reg = IntentRegistry::embedded().unwrap();
            let draft = IntentArtifact {
                type_id: "requirement.draft.requested".into(),
                slots: BTreeMap::new(),
                confidence: 0.9,
            };
            assert_eq!(apply_risk_gate(&reg, &draft, false), RiskGateResult::Allow);

            let run = IntentArtifact {
                type_id: "experiment.run.requested".into(),
                slots: BTreeMap::new(),
                confidence: 0.9,
            };
            assert_eq!(
                apply_risk_gate(&reg, &run, false),
                RiskGateResult::NeedsConfirmation
            );
            assert_eq!(apply_risk_gate(&reg, &run, true), RiskGateResult::Allow);

            let order = IntentArtifact {
                type_id: "order.live.submit.requested".into(),
                slots: BTreeMap::new(),
                confidence: 0.9,
            };
            assert_eq!(
                apply_risk_gate(&reg, &order, false),
                RiskGateResult::Forbidden
            );
            assert_eq!(apply_risk_gate(&reg, &order, true), RiskGateResult::Forbidden);
        });
    }

    #[test]
    fn offline_matrix_six_groups() {
        nautilus_research_testkit::case("UT-S24-06", || {
            let reg = IntentRegistry::embedded().unwrap();
            let cases: Vec<(&str, &str, fn(&TriageResult) -> bool)> = vec![
                (
                    "域内变体",
                    r#"{"domain":"in_scope","candidates":[{"type_id":"experiment.run.requested","slots":{},"confidence":0.95}]}"#,
                    |r| matches!(r, TriageResult::Artifact(a) if a.type_id == "experiment.run.requested"),
                ),
                (
                    "否定句",
                    r#"{"domain":"in_scope","candidates":[{"type_id":"experiment.run.requested","slots":{},"confidence":0.55},{"type_id":"boundary.explain.requested","slots":{},"confidence":0.52}]}"#,
                    |r| matches!(r, TriageResult::ClarificationRequested(_)),
                ),
                (
                    "歧义句",
                    r#"{"domain":"in_scope","candidates":[{"type_id":"design.generate.requested","slots":{},"confidence":0.72},{"type_id":"boundary.explain.requested","slots":{},"confidence":0.60}]}"#,
                    |r| matches!(r, TriageResult::ClarificationRequested(_)),
                ),
                (
                    "域外句",
                    r#"{"domain":"out_of_scope","candidates":[]}"#,
                    |r| *r == TriageResult::ChatFree,
                ),
                (
                    "高危越权",
                    r#"{"domain":"in_scope","candidates":[{"type_id":"order.live.submit.requested","slots":{"symbol":"600519"},"confidence":0.95}]}"#,
                    |r| matches!(r, TriageResult::Artifact(a) if a.type_id == "order.live.submit.requested"),
                ),
                (
                    "注入尝试",
                    r#"{"domain":"in_scope","candidates":[{"type_id":"plan.export.requested","slots":{},"confidence":0.95,"risk_class":"local_action"}]}"#,
                    |r| matches!(r, TriageResult::Artifact(a) if a.type_id == "plan.export.requested"),
                ),
            ];
            for (name, reply, pred) in cases {
                let result = parse_and_reduce_triage_reply(&reg, reply, 0.7, 0.15, &[]);
                assert!(
                    pred(&result),
                    "{name} 归约结果不符合预期：{result:?}"
                );
            }
        });
    }
}
