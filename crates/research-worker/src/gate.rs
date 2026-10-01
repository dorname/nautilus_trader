//! A股交易规则门：实现在领域核心（`nautilus_research_domain::gate`），
//! 此处 re-export 保持工作器侧的既有引用路径。

pub use nautilus_research_domain::gate::{
    gate_open_price, size_buy, validate_buy_qty, validate_sell_qty, BoardRules, Reject,
};
