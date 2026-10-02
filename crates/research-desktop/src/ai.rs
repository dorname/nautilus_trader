//! 离线预设意图对话引擎（S17）：关键词路由 → 预设意图与动作提示。
//!
//! 边界（架构 core-05「离线预设意图」）：无 LLM、无网络；未知意图诚实
//! 解释本助手的能力边界，不编造理解。

/// 预设意图（对话引擎可执行的动作集合）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// 确认需求（生成需求版本 R）。
    ConfirmRequirement,
    /// 新建项目。
    NewProject,
    /// 切换项目（数字参数由调用方解析）。
    SwitchProject,
    /// 生成设计说明（D 草稿）。
    GenerateDesign,
    /// 保存代码版本（不可变 v）。
    SaveVersion,
    /// 运行实验。
    RunExperiment,
    /// 比较实验。
    CompareExperiments,
    /// 生成验证报告。
    MakeReport,
    /// 生成交易计划。
    PlanGenerate,
    /// 核对计划。
    PlanCheck,
    /// 导出计划 CSV。
    PlanExport,
    /// 未知意图（诚实解释能力边界）。
    Unknown,
}

impl Intent {
    /// 意图对应的页面（工作台四页内跳转提示）。
    pub fn target_page(self) -> &'static str {
        match self {
            Intent::ConfirmRequirement | Intent::NewProject | Intent::SwitchProject => "项目",
            Intent::GenerateDesign | Intent::SaveVersion => "开发",
            Intent::RunExperiment | Intent::CompareExperiments | Intent::MakeReport => "调试",
            Intent::PlanGenerate | Intent::PlanCheck | Intent::PlanExport => "计划桥",
            Intent::Unknown => "项目",
        }
    }
}

/// 关键词路由（预设意图表；离线确定性）。
pub fn route(text: &str) -> Intent {
    let t = text.trim();
    if t.is_empty() {
        return Intent::Unknown;
    }
    if t.contains("确认需求") || t.contains("确认版本") {
        Intent::ConfirmRequirement
    } else if (t.contains("新建") || t.contains("新项目")) && t.contains("项目") {
        Intent::NewProject
    } else if t.contains("切换项目") || t.contains("切换到项目") {
        Intent::SwitchProject
    } else if t.contains("设计") {
        Intent::GenerateDesign
    } else if t.contains("保存版本") || t.contains("冻结版本") {
        Intent::SaveVersion
    } else if t.contains("运行实验") || t.contains("跑实验") || t.contains("运行回测") {
        Intent::RunExperiment
    } else if t.contains("比较") {
        Intent::CompareExperiments
    } else if t.contains("验证") || t.contains("报告") {
        Intent::MakeReport
    } else if t.contains("生成计划") {
        Intent::PlanGenerate
    } else if t.contains("核对") {
        Intent::PlanCheck
    } else if t.contains("导出") {
        Intent::PlanExport
    } else {
        Intent::Unknown
    }
}

/// 助手回复：预设文本 + 目标页提示（离线；不编造理解）。
pub fn reply(text: &str) -> String {
    match route(text) {
        Intent::ConfirmRequirement => {
            "请在项目页填写研究目标与验收标准后点击确认；投入比例须为 0～100%，成交额须为非负数。确认后生成需求版本 R，旧实验保留。".into()
        }
        Intent::NewProject => "已在项目页新建独立项目：会话、需求与版本互不串扰。".into(),
        Intent::SwitchProject => "请在项目页左侧列表选择要切换的项目；切回时版本与消息保留。".into(),
        Intent::GenerateDesign => {
            "请先确认需求版本 R，再在开发页生成设计说明；设计绑定当前需求版本。".into()
        }
        Intent::SaveVersion => {
            "请先检查代码草稿与设计一致，再在开发页保存版本 v；版本冻结引用 R/D，不可变。".into()
        }
        Intent::RunExperiment => {
            "请在调试页运行当前版本实验；上游（需求/设计/输入）更新后旧版本不可新运行，历史实验不受影响。".into()
        }
        Intent::CompareExperiments => {
            "请在调试页选择实验比较：同输入实验分歧归因代码；跨输入实验列出输入差异，不归因代码。".into()
        }
        Intent::MakeReport => {
            "验证报告按当前版本与实验证据生成：演示检查可能通过，正式策略验证始终为「证据不足」——真实数据、样本外与参数稳健性未运行。".into()
        }
        Intent::PlanGenerate => {
            "请在计划桥页生成交易计划：计划使用独立参考快照，绑定策略版本并冻结账户输入。".into()
        }
        Intent::PlanCheck => {
            "核对将检查：版本与验证报告未过期、快照与交易日匹配、现金充足、可卖数量与 100 股整数倍；核对通过后才能确认导出。".into()
        }
        Intent::PlanExport => {
            "仅核对通过且明确确认后可导出 CSV；文件含演示标识、策略版本、交易日与调整数量，不发送任何订单。".into()
        }
        Intent::Unknown => {
            "我是研究助手，只能处理预设的研究动作：确认需求、生成设计、保存版本、运行/比较实验、验证报告、计划生成/核对/导出。其他请求我无法理解，也不会假装理解。".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 意图路由的关键词覆盖（含未知意图诚实解释）。
    #[test]
    fn routes_and_unknown_honesty() {
        assert_eq!(route("请帮我确认需求"), Intent::ConfirmRequirement);
        assert_eq!(route("新建一个项目"), Intent::NewProject);
        assert_eq!(route("切换项目"), Intent::SwitchProject);
        assert_eq!(route("生成设计说明"), Intent::GenerateDesign);
        assert_eq!(route("保存版本 v2"), Intent::SaveVersion);
        assert_eq!(route("运行实验"), Intent::RunExperiment);
        assert_eq!(route("比较两次实验"), Intent::CompareExperiments);
        assert_eq!(route("生成验证报告"), Intent::MakeReport);
        assert_eq!(route("生成计划"), Intent::PlanGenerate);
        assert_eq!(route("核对计划"), Intent::PlanCheck);
        assert_eq!(route("导出 CSV"), Intent::PlanExport);
        assert_eq!(route("今天天气怎么样"), Intent::Unknown);
        assert_eq!(route(""), Intent::Unknown);
        // 未知意图不编造：回复解释能力边界
        assert!(reply("明天买什么股票").contains("无法理解"));
        // 每个意图都有目标页与预设回复
        for (t, page) in [
            ("确认需求", "项目"),
            ("保存版本", "开发"),
            ("运行实验", "调试"),
            ("核对", "计划桥"),
        ] {
            assert_eq!(route(t).target_page(), page);
            assert!(!reply(t).is_empty());
        }
    }
}
