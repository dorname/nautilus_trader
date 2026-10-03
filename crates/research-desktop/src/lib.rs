//! 研究桌面 GUI（研序）：统一工作台（左侧栏唯一导航 / 主区工作区画布 / 右对话栏）。
//!
//! 高保真还原 core-05-ai-workspace-prototype.html：左侧栏 212（窄窗 178）玻璃卡
//! 承载品牌、项目选择与两组 11 路由（研究工作区 8 + 研究资源 3）；主区玻璃大卡
//! 为 65px 顶栏（面包屑 + 演示环境 + 运行记录）、工作区画布 + 右对话栏
//! 360（窄窗 320）、28px footer（本地原型与溯源声明）；专注模式收起对话栏。
//! 右对话栏为离线预设意图引擎（ai.rs）：气泡、产物卡跳转、任务条、建议
//! chips 与诚实边界声明。
//!
//! 落地边界（架构 core-05）：GUI 只发类型化消息、接收不可变视图；
//! 引擎对象（Rc/RefCell，非 Send）不跨线程；按需重绘不打爆 CPU
//! （静默零帧，仅活跃任务以 POLL_INTERVAL 轮询）。

pub mod ai;
pub mod app;
pub mod bridge;
pub mod layout;
pub mod nav;
pub mod pipeline;
pub mod session;
pub mod theme;
pub mod workspace;
