//! 统一工作台布局断点与面板尺寸（纯函数，UT-S15-08 承载）。
//!
//! 对齐 core-05-ai-workspace-prototype.html：左侧栏 212（≤1200 窄窗 178）、
//! 右侧对话栏 360（窄窗 320）、顶栏 65、footer 28；外层留白与卡片间隙 12；
//! 专注模式收起对话栏；最小窗口 1100 下工作区无水平溢出。

use crate::theme::MIN_WINDOW;

/// 左侧栏宽度（舒适档）。
pub const SIDEBAR_W: f32 = 212.0;
/// 左侧栏宽度（窄窗 ≤1200）。
pub const SIDEBAR_W_NARROW: f32 = 178.0;
/// 右侧对话栏宽度（舒适档）。
pub const CHAT_W: f32 = 360.0;
/// 右侧对话栏宽度（窄窗 ≤1200）。
pub const CHAT_W_NARROW: f32 = 320.0;
/// 主卡片顶栏高度（面包屑 + 演示环境 tag）。
pub const TOPBAR_H: f32 = 65.0;
/// 主卡片 footer 高度（溯源声明）。
pub const FOOTER_H: f32 = 28.0;
/// 外层留白 / 卡片间隙（原型 .app gap 与 padding）。
pub const APP_GAP: f32 = 12.0;
/// 主卡片圆角（原型 16px；面板内子卡片 14px 由 theme::CARD_ROUNDING 承载）。
pub const CARD_RADIUS: f32 = 16.0;
/// 窄窗断点（原型 @media max-width:1200px；最小窗口 1100 落在窄档）。
pub const NARROW_MAX_W: f32 = 1200.0;
/// 图标栏断点（原型 @media max-width:900px：侧栏收成 65px 纯图标栏）。
pub const SLIM_MAX_W: f32 = 900.0;

/// 首帧守卫宽度阈值：WSLg 等环境 winit 首帧可能返回远小于请求值的窗口尺寸
/// （实测约 260×267，`with_inner_size`/`with_min_inner_size` 均未生效），
/// 低于阈值的帧跳过绘制，等待尺寸就绪（见 app::ui）。
pub const FIRST_FRAME_MIN_W: f32 = 400.0;
/// 首帧守卫高度阈值（与 FIRST_FRAME_MIN_W 同源）。
pub const FIRST_FRAME_MIN_H: f32 = 200.0;
/// 侧栏宽度（图标档 ≤900，原型 @900 .app grid-template-columns:65px）。
pub const SIDEBAR_W_SLIM: f32 = 65.0;
/// 对话栏宽度（图标档 ≤900，原型 @900 .body 280px）。
pub const CHAT_W_SLIM: f32 = 280.0;

/// 布局档位：由窗口宽度决定侧栏/对话栏宽度（纯函数，可测）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutPlan {
    /// 舒适档（>1200）：侧栏 212、对话栏 360。
    Comfortable,
    /// 窄档（901~1200）：侧栏 178、对话栏 320。
    Compact,
    /// 图标档（≤900）：侧栏 65 纯图标、对话栏 280（原型 @media max-width:900px）。
    Slim,
}

/// 按可用宽度决定布局档位。
pub fn plan_for_width(w: f32) -> LayoutPlan {
    if w > NARROW_MAX_W {
        LayoutPlan::Comfortable
    } else if w > SLIM_MAX_W {
        LayoutPlan::Compact
    } else {
        LayoutPlan::Slim
    }
}

/// 当前档位的侧栏宽度。
pub fn sidebar_w(plan: LayoutPlan) -> f32 {
    match plan {
        LayoutPlan::Comfortable => SIDEBAR_W,
        LayoutPlan::Compact => SIDEBAR_W_NARROW,
        LayoutPlan::Slim => SIDEBAR_W_SLIM,
    }
}

/// 当前档位的对话栏宽度。
pub fn chat_w(plan: LayoutPlan) -> f32 {
    match plan {
        LayoutPlan::Comfortable => CHAT_W,
        LayoutPlan::Compact => CHAT_W_NARROW,
        LayoutPlan::Slim => CHAT_W_SLIM,
    }
}

/// 工作区可用宽度（窗口 − 外留白 − 卡片间隙 − 侧栏 − 对话栏[专注模式收起]；负值钳 0）。
pub fn workspace_width(window_w: f32, plan: LayoutPlan, focus: bool) -> f32 {
    let mut avail = window_w - APP_GAP * 2.0 - APP_GAP - sidebar_w(plan);
    if !focus {
        avail -= chat_w(plan);
    }
    avail.max(0.0)
}

/// 最小窗口（1100，窄档，含对话栏）下工作区仍有可用宽度（无水平溢出的下界自检）。
pub fn min_workspace_usable() -> bool {
    workspace_width(MIN_WINDOW.x, plan_for_width(MIN_WINDOW.x), false) > 0.0
}

/// 首帧守卫判据：根矩形达到可绘制下限才进入布局（纯函数，UT-S15-08 承载）。
/// 守卫的意义：极端窄帧下 `可用宽 − 侧栏 − 对话栏` 必为负，而 egui 的
/// `allocate_ui` 对负期望尺寸直接断言 panic（desktop-firstframe-guard）。
pub fn frame_ready(w: f32, h: f32) -> bool {
    w >= FIRST_FRAME_MIN_W && h >= FIRST_FRAME_MIN_H
}

/// 根三列切分（desktop-root-layout）：sidebar 固定宽 + 间隙 + main 剩余。
/// 返回 (sidebar_w, main_w)；各段非负、和 + APP_GAP ≤ outer_w。
/// 侧栏宽按档位取 212/178（原型 .app grid-template-columns）。
pub fn columns(outer_w: f32, plan: LayoutPlan) -> (f32, f32) {
    let sidebar = sidebar_w(plan);
    let main = (outer_w - sidebar - APP_GAP).max(0.0);
    (sidebar, main)
}

/// 主卡三段切分（原型 .main grid-template-rows: 65px 1fr 28px）。
/// 返回 (topbar, body, footer)；各段非负、和 ≤ outer_h。
pub fn rows(outer_h: f32) -> (f32, f32, f32) {
    let body = (outer_h - TOPBAR_H - FOOTER_H).max(0.0);
    (TOPBAR_H, body, FOOTER_H)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S15-08（断言之组三）：断点档位、宽度常量与最小窗口无水平溢出。
    #[test]
    fn breakpoints_and_min_workspace() {
        // 档位切换：>1200 舒适 / ≤1200 窄（1100 最小窗口落窄档）
        assert_eq!(plan_for_width(1440.0), LayoutPlan::Comfortable);
        assert_eq!(plan_for_width(1201.0), LayoutPlan::Comfortable);
        assert_eq!(plan_for_width(1200.0), LayoutPlan::Compact);
        assert_eq!(plan_for_width(1100.0), LayoutPlan::Compact);
        // 宽度常量与原型一致
        assert_eq!(SIDEBAR_W, 212.0);
        assert_eq!(SIDEBAR_W_NARROW, 178.0);
        assert_eq!(CHAT_W, 360.0);
        assert_eq!(CHAT_W_NARROW, 320.0);
        assert_eq!(TOPBAR_H, 65.0);
        assert_eq!(FOOTER_H, 28.0);
        assert_eq!(APP_GAP, 12.0);
        assert_eq!(CARD_RADIUS, 16.0);
        // 侧栏/对话栏宽度随档位
        assert_eq!(sidebar_w(LayoutPlan::Comfortable), 212.0);
        assert_eq!(sidebar_w(LayoutPlan::Compact), 178.0);
        assert_eq!(chat_w(LayoutPlan::Comfortable), 360.0);
        assert_eq!(chat_w(LayoutPlan::Compact), 320.0);
        // 工作区宽度：1440 舒适 = 1440-24-12-212-360 = 832
        assert!(
            (workspace_width(1440.0, LayoutPlan::Comfortable, false) - 832.0).abs() < 1e-6,
            "1440 舒适档工作区应为 832"
        );
        // 1100 窄档 = 1100-24-12-178-320 = 566
        assert!(
            (workspace_width(1100.0, LayoutPlan::Compact, false) - 566.0).abs() < 1e-6,
            "1100 窄档工作区应为 566"
        );
        // 专注模式收起对话栏：+360 / +320
        assert!(
            (workspace_width(1440.0, LayoutPlan::Comfortable, true) - 1192.0).abs() < 1e-6,
            "专注模式工作区应为 1192"
        );
        // 负值钳 0
        assert_eq!(workspace_width(100.0, LayoutPlan::Comfortable, false), 0.0);
        // 最小窗口下工作区可用（无水平溢出）
        assert!(min_workspace_usable());
    }
}
