//! 会话状态：当前路由与每路由滚动位置（页面切换互不干扰；落盘持久化在
//! 对接协调器会话存储时实现，本批仅内存态）。

use crate::nav::{default_route, Route};

/// 统一工作台会话状态。
#[derive(Debug, Clone)]
pub struct Session {
    /// 当前路由。
    pub route: Route,
    /// 每路由滚动位置（切换返回时保留，按需重绘不依赖滚动状态）。
    scroll: [f32; 11],
    /// 字体提示（系统 CJK 字体缺失时状态行提示）。
    pub font_missing: bool,
}

impl Session {
    /// 新会话：默认路由 + 空滚动 + 字体正常。
    pub fn new() -> Self {
        Self {
            route: default_route(),
            scroll: [0.0; 11],
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

    /// 切路由：保留来源路由滚动位置（会话状态纯逻辑，UT 承载）。
    pub fn navigate(&mut self, to: Route, source_scroll: f32) {
        let from = self.route.index();
        self.scroll[from] = source_scroll;
        self.route = to;
    }

    /// 读取当前路由滚动位置。
    pub fn scroll_of(&self, route: Route) -> f32 {
        self.scroll[route.index()]
    }

    /// 回写当前路由滚动位置（GUI 每帧回写；切路由时由 `navigate` 冻结来源值）。
    pub fn record_scroll(&mut self, route: Route, y: f32) {
        self.scroll[route.index()] = y;
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

    /// UT-S15-07（断言之组二）：切路由保留来源滚动、目标路由独立恢复。
    #[test]
    fn navigate_keeps_scroll_per_route() {
        let mut s = Session::new();
        assert_eq!(s.route, Route::Overview);
        s.navigate(Route::Experiments, 42.0);
        assert_eq!(s.route, Route::Experiments);
        assert!((s.scroll_of(Route::Overview) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Route::Experiments) - 0.0).abs() < 1e-6);
        s.navigate(Route::Overview, 7.0);
        assert_eq!(s.route, Route::Overview);
        assert!((s.scroll_of(Route::Overview) - 42.0).abs() < 1e-6);
        assert!((s.scroll_of(Route::Experiments) - 7.0).abs() < 1e-6);
        // 研究资源组路由同样独立
        s.navigate(Route::Data, 5.0);
        assert_eq!(s.route, Route::Data);
        assert!((s.scroll_of(Route::Data) - 0.0).abs() < 1e-6);
    }
}
