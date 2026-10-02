//! 会话状态：当前页与每页占位状态（页面切换互不干扰；落盘持久化在 L3/L4
//! 对接协调器会话存储时实现，本批仅内存态）。

use crate::nav::{default_page, Page};

/// 桌面会话状态。
#[derive(Debug, Clone)]
pub struct Session {
    /// 当前页。
    pub page: Page,
    /// 每页滚动位置（切换返回时保留，按需重绘不依赖滚动状态）。
    scroll: [f32; 6],
    /// 字体提示（系统 CJK 字体缺失时状态行提示）。
    pub font_missing: bool,
}

impl Session {
    /// 新会话：默认页 + 空滚动 + 字体正常。
    pub fn new() -> Self {
        Self {
            page: default_page(),
            scroll: [0.0; 6],
            font_missing: false,
        }
    }

    /// 新会话并注入字体缺失提示（App 层入口）。
    pub fn with_font_missing(font_missing: bool) -> Self {
        Self {
            font_missing,
            ..Self::new()
        }
    }

    /// 切页：保留来源页滚动位置（会话状态纯逻辑，UT 承载）。
    pub fn navigate(&mut self, to: Page, source_scroll: f32) {
        let from = self.page.index();
        self.scroll[from] = source_scroll;
        self.page = to;
    }

    /// 读取当前页滚动位置。
    pub fn scroll_of(&self, page: Page) -> f32 {
        self.scroll[page.index()]
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::Page;

    /// UT-S15-07（断言之三）：切页保留来源页滚动、目标页从既有位置恢复。
    #[test]
    fn navigate_keeps_scroll_per_page() {
        let mut s = Session::new();
        assert_eq!(s.page, Page::DataSnapshot);
        s.navigate(Page::Runs, 42.0);
        assert_eq!(s.page, Page::Runs);
        assert!((s.scroll_of(Page::DataSnapshot) - 42.0).abs() < 1e-6);
        // 目标页保留自己此前的位置（新页为 0）
        assert!((s.scroll_of(Page::Runs) - 0.0).abs() < 1e-6);
        s.navigate(Page::DataSnapshot, 7.0);
        assert_eq!(s.page, Page::DataSnapshot);
        assert!((s.scroll_of(Page::DataSnapshot) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Page::Runs) - 7.0).abs() < 1e-6);
    }
}
