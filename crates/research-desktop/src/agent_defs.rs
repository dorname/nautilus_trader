//! Agent 定义清单（core-06 §4；issue #13）。
//!
//! `logos/agents/*.toml` 为权威声明源，经 `include_str!` 编译期内嵌——
//! 运行时不依赖仓库文件。派发契约声明 `definition_id` 时必须命中清单，
//! 未知 id 拒绝派发（工具白名单约束力落在客户端校验，octos stdio 侧无
//! 注册 API，见 issue #16/#20 能力子集说明）。
use serde::Deserialize;

/// 内嵌 manifest（logos/agents/ 为权威源，改动需同步本表）。
const MANIFESTS: [&str; 4] = [
    include_str!("../../../logos/agents/requirement-analyst.toml"),
    include_str!("../../../logos/agents/design-generator.toml"),
    include_str!("../../../logos/agents/code-reviewer.toml"),
    include_str!("../../../logos/agents/experiment-runner.toml"),
];

/// Agent 定义（core-06 §4 schema，version=1）。
#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefinition {
    /// 定义 ID（派发契约 definition_id 引用此名）。
    pub name: String,
    /// schema 版本（恒 1）。
    pub version: u32,
    /// 职责摘要。
    pub summary: String,
    /// 工具白名单。
    #[serde(default)]
    pub tools: Vec<String>,
    /// 工具黑名单（优先于白名单）。
    #[serde(default)]
    pub disallowed_tools: Vec<String>,
}

fn definitions() -> &'static [AgentDefinition] {
    use std::sync::OnceLock;
    static DEFS: OnceLock<Vec<AgentDefinition>> = OnceLock::new();
    DEFS.get_or_init(|| {
        MANIFESTS
            .iter()
            .map(|src| toml::from_str::<AgentDefinition>(src).expect("内嵌 manifest 必须可解析"))
            .collect()
    })
}

/// 全部内嵌定义。
pub fn agent_definitions() -> &'static [AgentDefinition] {
    definitions()
}

/// 按名查询定义。
pub fn agent_definition(name: &str) -> Option<&'static AgentDefinition> {
    definitions().iter().find(|d| d.name == name)
}
