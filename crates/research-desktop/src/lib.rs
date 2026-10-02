//! 研究桌面 GUI（研序）：六页导航骨架与黑色玻璃态主题。
//!
//! 批次 L2 范围（astock-desktop-launch 提案）：依赖接入、六页导航骨架、
//! 主题/布局/会话状态纯函数与按需重绘约束。流水线五页的业务对接在 L3，
//! AI 工作台四页在 L4——本批页面主体只渲染占位说明，不伪造业务状态。
//!
//! 落地边界（架构 core-05）：GUI 只发类型化消息、接收不可变视图；
//! 引擎对象（Rc/RefCell，非 Send）不跨线程；按需重绘不打爆 CPU。

pub mod ai;
pub mod app;
pub mod bridge;
pub mod layout;
pub mod nav;
pub mod pipeline;
pub mod session;
pub mod theme;
pub mod workspace;
