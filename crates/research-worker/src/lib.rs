//! A股研究运行工作器：真实 Nautilus 引擎编排（S13）。
//!
//! - `fees`：A股费用模型（佣金费率+最低、卖出印花税、其他费用）；
//! - `gate`：板块规则门（最小量/步长/涨跌停/T+1）与含费缩量；
//! - `runner`：日线 EMA 运行的引擎装配与规范化产物。
//!
//! 引擎对象含 Rc/RefCell：全部在同一线程创建、使用、销毁，不跨线程搬移。

pub mod adapter;
pub mod fees;
pub mod gate;
pub mod runner;
