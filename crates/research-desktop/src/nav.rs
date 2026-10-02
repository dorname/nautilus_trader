//! 六页导航路由：枚举、中文标题、顺序与默认页（纯函数，UT-S15-07 承载）。
//!
//! 六页 = 研究流水线五页（数据快照/股票池/运行/比较/计划）＋ AI 工作台；
//! 页面主体由批次 L3（流水线五页）与 L4（AI 工作台）填充。

/// 六页导航枚举（顺序即侧栏顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    /// 数据快照（导入暂存数据与快照清单）。
    DataSnapshot,
    /// 股票池（预览/保存与三态原因）。
    Universe,
    /// 运行（提交回测与任务状态）。
    Runs,
    /// 比较（2..5 次运行指标与差异清单）。
    Compare,
    /// 计划（生成/核对/导出交易计划）。
    Plan,
    /// AI 工作台（S17～S20，批次 L4）。
    AiWorkspace,
}

pub const ALL_PAGES: [Page; 6] = [
    Page::DataSnapshot,
    Page::Universe,
    Page::Runs,
    Page::Compare,
    Page::Plan,
    Page::AiWorkspace,
];

impl Page {
    /// 侧栏中文标题（与统一原型导航文案同源）。
    pub fn title(self) -> &'static str {
        match self {
            Page::DataSnapshot => "数据快照",
            Page::Universe => "股票池",
            Page::Runs => "运行",
            Page::Compare => "比较",
            Page::Plan => "计划",
            Page::AiWorkspace => "AI 工作台",
        }
    }

    /// 占位说明（批次 L3/L4 落地前的诚实占位，不伪造业务状态）。
    pub fn placeholder(self) -> &'static str {
        match self {
            Page::DataSnapshot => "数据导入与快照清单（批次 L3 对接协调器导入命令）",
            Page::Universe => "股票池预览/保存与三态原因（批次 L3 对接预览命令）",
            Page::Runs => "回测运行提交与任务状态（批次 L3 对接运行命令）",
            Page::Compare => "运行比较指标与差异清单（批次 L3 对接比较命令）",
            Page::Plan => "交易计划生成/核对/导出（批次 L3 对接计划命令）",
            Page::AiWorkspace => "AI 工作台：项目/开发/调试/计划桥（批次 L4）",
        }
    }

    /// 顺序号（导航次序稳定，落盘会话状态引用）。
    pub fn index(self) -> usize {
        ALL_PAGES.iter().position(|p| *p == self).expect("六页枚举完备")
    }

    /// 由顺序号恢复页面（会话状态恢复；越界回退默认页）。
    pub fn from_index(i: usize) -> Page {
        ALL_PAGES.get(i).copied().unwrap_or(default_page())
    }
}

/// 默认页：数据快照（研究链路起点）。
pub fn default_page() -> Page {
    Page::DataSnapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S15-07（断言之一）：六页枚举互异、标题非空、顺序号与枚举一致。
    #[test]
    fn pages_are_distinct_with_stable_titles() {
        let titles: Vec<_> = ALL_PAGES.map(|p| p.title()).to_vec();
        // 标题非空且互异
        for t in &titles {
            assert!(!t.is_empty());
        }
        for i in 0..titles.len() {
            for j in (i + 1)..titles.len() {
                assert_ne!(titles[i], titles[j], "第 {i} 与第 {j} 页标题重复");
            }
        }
        // 顺序号与位置一致，from_index 可往返
        for (i, p) in ALL_PAGES.iter().enumerate() {
            assert_eq!(p.index(), i);
            assert_eq!(Page::from_index(i), *p);
        }
    }

    /// UT-S15-07（断言之二）：越界顺序号回退默认页，默认页为数据快照。
    #[test]
    fn from_index_out_of_range_falls_back_to_default() {
        assert_eq!(default_page(), Page::DataSnapshot);
        assert_eq!(Page::from_index(6), Page::DataSnapshot);
        assert_eq!(Page::from_index(999), Page::DataSnapshot);
    }
}
