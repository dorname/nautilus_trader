//! 统一工作台路由：枚举、中文标题、两组导航与默认路由（纯函数，UT-S15-07 承载）。
//!
//! 对齐 core-05-ai-workspace-prototype.html 唯一导航：研究工作区 8 路由
//! （项目概览/需求文档/策略设计/策略开发/事件调试/回测实验/验证报告/交易计划）
//! ＋ 研究资源 3 路由（数据中心/股票池/策略资产）；旧六页能力迁入对应路由。

use crate::ai::Intent;

/// 统一工作台路由枚举（侧栏两组顺序即枚举顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Route {
    /// 项目概览（下一步、指标卡与研究路径）。
    Overview,
    /// 需求文档（研究目标、验收标准与确认历史）。
    Requirements,
    /// 策略设计（设计说明、参数与版本）。
    Design,
    /// 策略开发（源码草稿、不可变版本库）。
    Develop,
    /// 事件调试（实验运行、比较归因与证据）。
    Debug,
    /// 回测实验（实验记录、指标与比较）。
    Experiments,
    /// 验证报告（演示检查与正式验证边界）。
    Validate,
    /// 交易计划（计划输入、核对与确认导出）。
    Plan,
    /// 数据中心（导入与快照清单）。
    Data,
    /// 股票池（规则预览、保存与三态原因）。
    Pool,
    /// 策略资产（当前项目不可变版本库）。
    Library,
}

/// 研究工作区导航组（8 路由）。
pub const WORKSPACE_ROUTES: [Route; 8] = [
    Route::Overview,
    Route::Requirements,
    Route::Design,
    Route::Develop,
    Route::Debug,
    Route::Experiments,
    Route::Validate,
    Route::Plan,
];

/// 研究资源导航组（3 路由）。
pub const RESOURCE_ROUTES: [Route; 3] = [Route::Data, Route::Pool, Route::Library];

/// 全部路由（研究工作区组在前，研究资源组在后）。
pub const ALL_ROUTES: [Route; 11] = [
    Route::Overview,
    Route::Requirements,
    Route::Design,
    Route::Develop,
    Route::Debug,
    Route::Experiments,
    Route::Validate,
    Route::Plan,
    Route::Data,
    Route::Pool,
    Route::Library,
];

/// 导航组标题（侧栏 section-label 文案，与原型同源）。
pub const WORKSPACE_GROUP: &str = "研究工作区";
/// 导航组标题（侧栏 section-label 文案，与原型同源）。
pub const RESOURCE_GROUP: &str = "研究资源";

impl Route {
    /// 侧栏中文标题（与 core-05 原型 routes 表同源）。
    pub fn title(self) -> &'static str {
        match self {
            Route::Overview => "项目概览",
            Route::Requirements => "需求文档",
            Route::Design => "策略设计",
            Route::Develop => "策略开发",
            Route::Debug => "事件调试",
            Route::Experiments => "回测实验",
            Route::Validate => "验证报告",
            Route::Plan => "交易计划",
            Route::Data => "数据中心",
            Route::Pool => "股票池",
            Route::Library => "策略资产",
        }
    }

    /// 顺序号（导航次序稳定，落盘会话状态引用）。
    pub fn index(self) -> usize {
        ALL_ROUTES
            .iter()
            .position(|r| *r == self)
            .expect("11 路由枚举完备")
    }

    /// 由顺序号恢复路由（会话状态恢复；越界回退默认路由）。
    pub fn from_index(i: usize) -> Route {
        ALL_ROUTES.get(i).copied().unwrap_or(default_route())
    }

    /// 所属导航组：true = 研究工作区组，false = 研究资源组。
    pub fn in_workspace_group(self) -> bool {
        WORKSPACE_ROUTES.contains(&self)
    }

    /// 对话产物卡默认文案（原型 `tell()`：title 缺省为路由名，desc 缺省「点击查看产物」）。
    pub fn artifact_copy(self) -> (&'static str, &'static str) {
        (self.title(), "点击查看产物")
    }

    /// 对话意图 → 跳转路由（None = 不跳转，如未知意图）。
    pub fn from_intent(intent: Intent) -> Option<Route> {
        match intent {
            Intent::ConfirmRequirement | Intent::DraftRequirement => Some(Route::Requirements),
            Intent::NewProject | Intent::SwitchProject => Some(Route::Overview),
            Intent::GenerateDesign => Some(Route::Design),
            Intent::GenerateCode | Intent::SaveVersion => Some(Route::Develop),
            Intent::RunExperiment | Intent::CompareExperiments => Some(Route::Experiments),
            Intent::MakeReport | Intent::ExplainBoundary => Some(Route::Validate),
            Intent::PlanGenerate | Intent::PlanCheck | Intent::PlanExport => Some(Route::Plan),
            Intent::Unknown => None,
        }
    }
}

/// 默认路由：项目概览（统一工作台主入口，与原型一致）。
pub fn default_route() -> Route {
    Route::Overview
}

/// 欢迎产物卡标题（原型 createProject 首条 `title`）。
pub const WELCOME_CARD_TITLE: &str = "先写下你的研究想法";
/// 欢迎产物卡说明（原型 createProject 首条 `desc`）。
pub const WELCOME_CARD_DESC: &str = "需求草稿 · 等待确认";

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S15-07（断言之组一）：两组 11 路由与原型导航同源——标题互异、
    /// 组序稳定、顺序号往返一致、越界回退默认路由。
    #[test]
    fn routes_match_prototype_groups() {
        // 两组规模：研究工作区 8 + 研究资源 3
        assert_eq!(WORKSPACE_ROUTES.len(), 8);
        assert_eq!(RESOURCE_ROUTES.len(), 3);
        assert_eq!(ALL_ROUTES.len(), 11);
        // 标题非空且互异
        let titles: Vec<_> = ALL_ROUTES.map(|r| r.title()).to_vec();
        for (i, t) in titles.iter().enumerate() {
            assert!(!t.is_empty(), "第 {i} 路由标题为空");
            for other in titles.iter().skip(i + 1) {
                assert_ne!(t, other, "路由标题重复：{t}");
            }
        }
        // 与 core-05 原型 routes 表同序同文
        assert_eq!(
            titles,
            vec![
                "项目概览",
                "需求文档",
                "策略设计",
                "策略开发",
                "事件调试",
                "回测实验",
                "验证报告",
                "交易计划",
                "数据中心",
                "股票池",
                "策略资产",
            ]
        );
        // 组归属与分组常量一致
        for r in WORKSPACE_ROUTES {
            assert!(r.in_workspace_group());
        }
        for r in RESOURCE_ROUTES {
            assert!(!r.in_workspace_group());
        }
        // 顺序号往返
        for (i, r) in ALL_ROUTES.iter().enumerate() {
            assert_eq!(r.index(), i);
            assert_eq!(Route::from_index(i), *r);
        }
        // 默认路由与越界回退
        assert_eq!(default_route(), Route::Overview);
        assert_eq!(Route::from_index(11), Route::Overview);
        assert_eq!(Route::from_index(999), Route::Overview);
    }
}
