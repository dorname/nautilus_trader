//! 离线合成样本模拟（对齐 core-05 原型 `fixture` / `compute`）。
//!
//! 仅用于预置策略示例的确定性演示；不执行任意 Python。

use serde_json::json;

/// 合成行情样本（原型 `fixture`）。
#[derive(Debug, Clone, Copy)]
pub struct FixtureRow {
    pub symbol: &'static str,
    pub name: &'static str,
    pub market: &'static str,
    pub amount: f64,
    pub signal: bool,
    pub close: f64,
    pub open: f64,
    pub marks: [f64; 3],
}

/// 原型内置 3 个合成标的。
pub const FIXTURE: [FixtureRow; 3] = [
    FixtureRow {
        symbol: "SYN-A",
        name: "合成趋势样本",
        market: "沪市",
        amount: 2200.0,
        signal: true,
        close: 10.0,
        open: 10.1,
        marks: [10.0, 10.4, 10.8],
    },
    FixtureRow {
        symbol: "SYN-B",
        name: "合成反向样本",
        market: "深市",
        amount: 1600.0,
        signal: false,
        close: 20.0,
        open: 19.8,
        marks: [20.0, 19.9, 19.7],
    },
    FixtureRow {
        symbol: "SYN-C",
        name: "合成低流动性",
        market: "沪市",
        amount: 300.0,
        signal: false,
        close: 8.0,
        open: 8.1,
        marks: [8.0, 8.2, 8.3],
    },
];

/// 默认股票池成员（原型初始 `pool`）。
pub const DEFAULT_POOL: [&str; 2] = ["SYN-A", "SYN-B"];

/// 逐步事件（原型 `events[]`）。
#[derive(Debug, Clone)]
pub struct DemoEvent {
    pub seq: usize,
    pub node: String,
    pub symbol: String,
    pub status: String,
    pub input_json: String,
    pub output_json: String,
    pub line: u32,
    pub date: String,
    pub visible_at: String,
}

/// 一次合成运行结果（原型 `compute` 返回值）。
#[derive(Debug, Clone)]
pub struct DemoRun {
    pub events: Vec<DemoEvent>,
    pub equity: Vec<f64>,
    pub cash: f64,
    pub held: i64,
    pub rejected: u32,
    pub final_nav: f64,
    pub return_pct: f64,
    pub drawdown_pct: f64,
}

/// 运行参数（冻结版本上的设计与变体）。
#[derive(Debug, Clone)]
pub struct DemoInputs {
    pub variant: u8,
    pub allocation: f64,
    pub min_amount: f64,
    pub data_id: String,
    pub pool_id: String,
    pub pool: Vec<String>,
}

/// 对齐原型 `compute(v)` 的确定性演算。
pub fn compute(inputs: &DemoInputs) -> DemoRun {
    let mut events = Vec::new();
    let mut equity = vec![10_000.0];
    let mut cash = 10_000.0_f64;
    let mut held: i64 = 0;
    let mut rejected: u32 = 0;

    let mut add = |node: &str,
                   symbol: &str,
                   status: &str,
                   input: serde_json::Value,
                   output: serde_json::Value,
                   line: u32,
                   date: &str| {
        let visible_at = if date == "2026-01-05" {
            "2026-01-05 15:00".into()
        } else if node == "metric" {
            format!("{date} 15:00")
        } else {
            format!("{date} 09:30")
        };
        events.push(DemoEvent {
            seq: events.len() + 1,
            node: node.into(),
            symbol: symbol.into(),
            status: status.into(),
            input_json: input.to_string(),
            output_json: output.to_string(),
            line,
            date: date.into(),
            visible_at,
        });
    };

    add(
        "data",
        "全部",
        "已读取",
        json!({ "snapshot": inputs.data_id }),
        json!({ "visibleThrough": "2026-01-05 15:00", "count": 3 }),
        3,
        "2026-01-05",
    );

    for s in &FIXTURE {
        let in_pool = inputs.pool.iter().any(|x| x == s.symbol) && s.amount >= inputs.min_amount;
        add(
            "filter",
            s.symbol,
            if in_pool { "入选" } else { "排除" },
            json!({
                "amount": s.amount,
                "threshold": inputs.min_amount,
                "pool": inputs.pool_id,
            }),
            json!({ "included": in_pool }),
            4,
            "2026-01-05",
        );
        if !in_pool {
            continue;
        }
        add(
            "signal",
            s.symbol,
            if s.signal { "形成信号" } else { "无信号" },
            json!({ "close": s.close }),
            json!({ "signal": s.signal }),
            5,
            "2026-01-05",
        );
        if !s.signal || s.symbol != "SYN-A" {
            continue;
        }
        let fee_reserve = if inputs.variant == 2 { 5.0 } else { 0.0 };
        let reference = if inputs.variant == 2 { s.open } else { s.close };
        let quantity = (((cash * inputs.allocation - fee_reserve) / reference / 100.0).floor()
            * 100.0)
            .max(0.0) as i64;
        let cost = quantity as f64 * s.open + if quantity > 0 { 5.0 } else { 0.0 };
        add(
            "size",
            s.symbol,
            "计算完成",
            json!({
                "cash": cash,
                "allocation": inputs.allocation,
                "reference": reference,
                "fee": 5,
            }),
            json!({ "quantity": quantity, "cost": cost }),
            8,
            "2026-01-06",
        );
        if cost > cash {
            rejected += 1;
            add(
                "fill",
                s.symbol,
                "资金不足 · 拒绝",
                json!({ "quantity": quantity, "price": s.open, "cash": cash }),
                json!({ "accepted": false, "required": cost, "cash": cash, "held": 0 }),
                10,
                "2026-01-06",
            );
        } else {
            cash = ((cash - cost) * 100.0).round() / 100.0;
            held = quantity;
            add(
                "fill",
                s.symbol,
                if quantity > 0 { "已成交" } else { "无需成交" },
                json!({ "quantity": quantity, "price": s.open }),
                json!({ "accepted": true, "cash": cash, "held": held }),
                10,
                "2026-01-06",
            );
        }
    }

    for i in 1..3 {
        let close = FIXTURE[0].marks[i];
        let value = ((cash + held as f64 * close) * 100.0).round() / 100.0;
        equity.push(value);
        let date = format!("2026-01-0{}", 5 + i);
        add(
            "metric",
            "账户",
            "已估值",
            json!({ "cash": cash, "held": held, "close": close }),
            json!({ "equity": value }),
            11,
            &date,
        );
    }

    let final_nav = *equity.last().unwrap_or(&10_000.0);
    let return_pct = (final_nav / 10_000.0 - 1.0) * 100.0;
    let mut peak: f64 = equity[0];
    let mut dd: f64 = 0.0;
    for e in &equity {
        peak = peak.max(*e);
        if peak > 0.0 {
            dd = dd.max((peak - *e) / peak * 100.0);
        }
    }

    DemoRun {
        events,
        equity,
        cash,
        held,
        rejected,
        final_nav,
        return_pct,
        drawdown_pct: dd,
    }
}

/// 最近两次实验的首个输出分歧（原型 `firstDivergence`）。
pub fn first_divergence<'a>(
    a: &'a DemoRun,
    b: &'a DemoRun,
) -> Option<(&'a DemoEvent, Option<&'a DemoEvent>)> {
    for e in &a.events {
        let f = b.events.iter().find(|x| {
            x.node == e.node && x.symbol == e.symbol && x.date == e.date
        });
        match f {
            Some(f) if f.output_json == e.output_json => continue,
            other => return Some((e, other)),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(variant: u8) -> DemoInputs {
        DemoInputs {
            variant,
            allocation: 1.0,
            min_amount: 1000.0,
            data_id: "SYN-202601-r1".into(),
            pool_id: "U1".into(),
            pool: DEFAULT_POOL.iter().map(|s| (*s).into()).collect(),
        }
    }

    #[test]
    fn original_rejects_for_cash() {
        let r = compute(&inputs(1));
        assert_eq!(r.rejected, 1);
        assert_eq!(r.held, 0);
        assert!((r.cash - 10_000.0).abs() < 1e-6);
        assert!(r.events.iter().any(|e| e.status.contains("拒绝")));
    }

    #[test]
    fn fixed_fills_position() {
        let r = compute(&inputs(2));
        assert_eq!(r.rejected, 0);
        assert!(r.held > 0);
        assert!(r.cash < 10_000.0);
        assert!(r.final_nav > 0.0);
    }
}
