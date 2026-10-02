//! 布局断点与面板尺寸（纯函数，UT-S15-08 承载）。
//!
//! core-02 界面结构：左侧 180 逻辑像素导航，顶部 56 像素，底部 32 像素任务状态；
//! 1440 宽三栏可操作，1100 宽收窄为中栏（右侧栏收起）。

use crate::theme::MIN_WINDOW;

/// 左侧导航宽度。
pub const SIDEBAR_W: f32 = 180.0;
/// 顶栏高度。
pub const TOPBAR_H: f32 = 56.0;
/// 底部任务状态栏高度。
pub const BOTTOMBAR_H: f32 = 32.0;
/// 三栏布局最小宽度（低于则右侧栏收起）。
pub const THREE_COL_MIN_W: f32 = 1280.0;

/// 右侧栏宽度（批次 L3/L4 的详情/对话区）。
pub const RIGHT_W: f32 = 320.0;

/// 布局方案：右栏收起与否由窗口宽度决定（纯函数，可测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPlan {
    /// 宽窗口：左导航 + 中工作区 + 右详情。
    ThreeCol,
    /// 窄窗口（≥1100）：右栏收起，工作区全宽。
    TwoCol,
}

/// 按可用宽度决定布局方案；低于最小窗口宽度视为仍取两栏（不产生水平溢出）。
pub fn plan_for_width(w: f32) -> LayoutPlan {
    if w >= THREE_COL_MIN_W {
        LayoutPlan::ThreeCol
    } else {
        LayoutPlan::TwoCol
    }
}

/// 中栏可用宽度（减去左导航；右栏存在时再减去右宽，负值钳为 0）。
pub fn center_width(w: f32, plan: LayoutPlan) -> f32 {
    let mut avail = w - SIDEBAR_W;
    if plan == LayoutPlan::ThreeCol {
        avail -= RIGHT_W;
    }
    avail.max(0.0)
}

/// 最小窗口下两栏中栏仍可用（1100-180=920 ≥ 720，不产生水平溢出的下界自检）。
pub fn min_center_usable() -> bool {
    center_width(MIN_WINDOW.x, plan_for_width(MIN_WINDOW.x)) >= MIN_WINDOW.y
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S15-08（断言之三）：断点切换与最小宽度下无水平溢出。
    #[test]
    fn breakpoints_and_min_center() {
        assert_eq!(plan_for_width(1440.0), LayoutPlan::ThreeCol);
        assert_eq!(plan_for_width(1280.0), LayoutPlan::ThreeCol);
        assert_eq!(plan_for_width(1100.0), LayoutPlan::TwoCol);
        assert_eq!(plan_for_width(900.0), LayoutPlan::TwoCol);
        // 最小窗口下中栏可用（不溢出）
        assert!(min_center_usable());
        // 中栏宽度计算
        assert!((center_width(1440.0, LayoutPlan::ThreeCol) - (1440.0 - 180.0 - 320.0)).abs() < 1e-6);
        assert!((center_width(1100.0, LayoutPlan::TwoCol) - (1100.0 - 180.0)).abs() < 1e-6);
        assert_eq!(center_width(100.0, LayoutPlan::ThreeCol), 0.0);
    }
}
