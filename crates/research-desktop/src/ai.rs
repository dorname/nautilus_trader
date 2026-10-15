//! 离线预设意图对话引擎（S17/S24）：L0 显式命令 → 预设意图与动作提示。
//!
//! 边界（架构 core-05 / S24）：无 LLM、无网络；未知意图诚实解释本助手的
//! 能力边界，不编造理解。非显式输入进入 S24 Triage，不在这里做关键词分类。
//! 意图 → 路由跳转见 `crate::nav::Route::from_intent`。

pub use crate::intents::{Intent, route};

/// 自由对话派发决策（S22，online-chat-llm-config-20261012）。
/// S24 修订：Unknown 且 octos 可用时不再直接 LlmChat，而是先 Triage；
/// 此处保留枚举供兼容性使用，实际入口决策由 `app.rs` 的 L0/L1/L2 状态机处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatDispatch {
    /// 确定性业务动作：本地执行，不经 LLM。
    LocalAction,
    /// 自由问答：派发真实 LLM turn（经 octos）。
    LlmChat,
    /// octos 不可用：诚实降级为离线预设回复（UI 需如实标注）。
    OfflinePreset,
}

/// 意图路由兜底改道（UT-S22-02 纯函数锚点，S24 兼容口径）：
/// 确定性意图一律本地动作；Unknown 在 octos 可用时进入 Triage（此处仍标记为 LlmChat
/// 以保持既有返回类型），不可用时降级离线预设。
pub fn decide_chat_dispatch(intent: &Intent, octos_available: bool) -> ChatDispatch {
    match intent {
        Intent::Unknown => {
            if octos_available {
                ChatDispatch::LlmChat
            } else {
                ChatDispatch::OfflinePreset
            }
        }
        _ => ChatDispatch::LocalAction,
    }
}

/// 助手回复：预设文本 + 目标页提示（离线；不编造理解）。
pub fn reply(text: &str) -> String {
    match route(text) {
        Intent::ConfirmRequirement => {
            "请在需求文档页填写研究目标与验收标准后确认；投入比例须为 0～100%，成交额须为非负数。确认后生成需求版本 R，旧实验保留。".into()
        }
        Intent::DraftRequirement => {
            "需求草稿已整理到工作区。请补充研究范围、参数和验收标准，然后确认需求版本。".into()
        }
        Intent::NewProject => "已在左侧栏新建独立项目：会话、需求与版本互不串扰。".into(),
        Intent::SwitchProject => "请在左侧栏顶部项目选择器切换；切回时版本与消息保留。".into(),
        Intent::GenerateDesign => {
            "设计草稿已生成。两张图共用六个处理节点，请检查参数和说明后保存设计版本。".into()
        }
        Intent::GenerateCode => {
            "已生成可编辑的策略示例。先检查并保存 v1，再运行实验观察资金约束。此示例不在桌面中执行 Python。".into()
        }
        Intent::SaveVersion => {
            "请在策略开发页编辑并保存版本 v；版本冻结引用 R/D，不可变。运行引用已保存版本，草稿尚未生效。".into()
        }
        Intent::RunExperiment => {
            "请在事件调试/回测实验页运行当前版本实验；上游（需求/设计/输入）更新后旧版本不可新运行，历史实验不受影响。".into()
        }
        Intent::CompareExperiments => {
            "实验比较在回测实验页：同输入实验分歧归因代码；跨输入实验列出输入差异，不归因代码。".into()
        }
        Intent::MakeReport => {
            "验证报告按当前版本与实验证据生成：演示检查可能通过，正式策略验证始终为「证据不足」——真实数据、样本外与参数稳健性未运行。".into()
        }
        Intent::ExplainBoundary => {
            "它们不是同一件事。\n\n调试：解释信号、订单与代码为什么这样运行。\n回测：在历史数据与指定假设下模拟策略表现。\n策略验证：按研究标准综合正确性、样本外和稳健性证据。\n计划核对：检查这一次调整的现金、可卖数量和数据时效。\n\n回测是策略验证的一种手段。计划核对通过不保证盈利。".into()
        }
        Intent::PlanGenerate => {
            "请在交易计划页生成计划：计划使用独立参考快照，绑定策略版本并冻结账户输入。".into()
        }
        Intent::PlanCheck => {
            "核对将检查：版本与验证报告未过期、输入未改变、现金充足、可卖数量与 100 股整数倍；核对通过后才能确认导出。".into()
        }
        Intent::PlanExport => {
            "仅核对通过且明确确认后可导出 CSV；文件含演示标识、策略版本、交易日与调整数量，不发送任何订单。".into()
        }
        Intent::Unknown => {
            "当前是离线预设对话，尚不能理解任意指令。可以试试：确认研究需求、生成策略设计、保存代码版本、运行回测实验、生成验证报告、计划生成/核对/导出，或让我解释验证区别。你的消息已保留在当前项目。".into()
        }
    }
}

/// 自由对话 prompt 模板（S22）：项目上下文 + 用户消息 + 诚实边界约束
/// （不编造实验数值——数值只来自协调器任务）。
pub fn chat_prompt(
    project_name: &str,
    req_summary: &str,
    design_summary: &str,
    user_text: &str,
) -> String {
    format!(
        "你是量化研究桌面「AI 工作台」的中文助手，正在协助用户做量化策略研究。\n\
         当前项目：{project_name}\n当前需求摘要：{req_summary}\n当前设计摘要：{design_summary}\n\
         请结合项目上下文简洁、诚实地用中文回答；不要编造实验数值\
         （实验数值只来自协调器任务）。\n\n用户消息：{user_text}"
    )
}

/// 需求草稿 prompt 模板（S23 / issue #39）：从用户自然语言研究想法
/// 隐式生成结构化需求草稿（草稿只入草稿框，版本确认由用户显式执行）。
pub fn requirement_draft_prompt(project_name: &str, user_text: &str) -> String {
    format!(
        "你是量化研究桌面「AI 工作台」的中文助手。请把用户描述的研究想法整理成一份\
         结构化研究需求草稿，用中文 Markdown 小节输出，包含：\n\
         1. 研究目标（要验证的策略假设）\n\
         2. 研究范围（市场/标的/数据频率与时间区间）\n\
         3. 策略参数（名称与默认值）\n\
         4. 验收标准（可判定的通过条件）\n\
         5. 风险与边界说明\n\
         只输出草稿正文，不要输出额外解释；不确定的字段给出合理默认并标注「待确认」。\
         不要编造回测数值。\n\n当前项目：{project_name}\n用户消息：{user_text}"
    )
}

/// Triage 结构化转译 prompt（S24）：输入注册表、项目摘要、用户原话；
/// 输出 JSON：`{"domain":"in_scope","candidates":[{"type_id":..., "slots":{}, "confidence":0.9}]}`。
/// 不包含用户原文/槽位值的上下文要求模型按 schema 修正。
pub fn triage_prompt(
    registry_text: &str,
    project_name: &str,
    req_summary: &str,
    user_text: &str,
    prior_validation_error: Option<&str>,
) -> String {
    let correction = prior_validation_error
        .map(|e| format!("\n上一次输出未通过 schema 校验，请修正：{e}\n"))
        .unwrap_or_default();
    format!(
        "你是量化研究桌面「AI 工作台」的意图识别助手。请根据以下意图注册表、项目状态和用户消息，把用户意图转译为结构化候选。\
         只输出一个 JSON 对象，不要 Markdown、不要解释。\
         \n\n意图注册表（id / description / risk_class / boundary_negative / slots）：\
         \n{registry_text}\n\
         \n项目状态：\
         \n- 项目名：{project_name}\n- 当前需求摘要：{req_summary}\n\
         \n用户消息：{user_text}\n\
         {correction}\n\
         输出 schema：\n\
         ```json\n{{\"domain\":\"in_scope\",\"candidates\":[{{\"type_id\":\"来自注册表的合法id\",\"slots\":{{}},\"confidence\":0.0到1.0}}]}}\n```\n\
         规则：\
         1. domain 只能是 in_scope 或 out_of_scope；out_of_scope 时 candidates 必须为空。\
         2. candidates 最多 3 个，按置信度降序排列；confidence 必须 ∈[0,1]。\
         3. type_id 必须来自注册表；slots 只能使用对应类型声明过的槽位；未声明槽位不得出现。\
         4. 真实下单、买入卖出等交易请求必须归为 order.live.submit.requested（forbidden）。\
         5. 否定句、先别做某事等不得直接给出 needs_confirm 执行的 confident 候选；应降低 confidence 或 out_of_scope。\
         6. 当用户消息是闲聊、问候、天气、时间、个人事务、与量化研究无关的任意指令，或明确要求你自由回答时，domain 必须为 out_of_scope 且 candidates 为空。\
         7. 用户表达可能对应多个意图时（如「帮我看看设计」既可理解为生成设计，也可理解为解释边界），必须给出 2~3 个相关候选并降低单个置信度，使 top1-top2 差距小于 0.15，触发澄清。\
         8. 对于明确指向单一业务意图的口语变体（如「帮我跑一下」明显对应 experiment.run.requested），只给出 1 个高置信度候选（confidence ≥ 0.85），不要过度澄清。"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S24-02：L0 仅精确命中。
    #[test]
    fn routes_and_unknown_honesty() {
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
        // 未知意图不编造：回复解释能力边界
        assert!(reply("明天买什么股票").contains("不能理解"));
        // 每个意图都有预设回复
        for t in [
            "确认需求",
            "生成需求草稿",
            "新建项目",
            "切换项目",
            "生成设计",
            "生成策略代码",
            "保存版本",
            "运行实验",
            "比较",
            "生成验证报告",
            "解释验证区别",
            "生成计划",
            "核对",
            "导出",
        ] {
            assert!(reply(t).len() > 8, "每个意图都有预设回复：{t}");
        }
    }
}
