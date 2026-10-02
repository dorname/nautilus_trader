//! 命令行参数解析（手写薄解析器：`--flag 值` / 布尔 flag；中文诊断）。
//!
//! 设计约束（规格 core-S21-cli-research.md）：CLI 是协调器之上的薄壳，
//! 解析失败返回中文用法错误并以退出码 3 拒绝，不触达协调器。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// 退出码契约（与 worker 对齐）：0 成功 / 3 业务拒绝（含用法错误）/ 4 环境错误。
pub const EXIT_OK: i32 = 0;
pub const EXIT_REJECTED: i32 = 3;
pub const EXIT_ENV: i32 = 4;

/// 全局参数。
#[derive(Debug, Clone)]
pub struct Global {
    pub workspace: PathBuf,
    pub json: bool,
}

/// 策略参数（run submit 与 plan generate 共用）。
#[derive(Debug, Clone)]
pub struct StrategyArgs {
    pub template: String,
    pub version: String,
    pub fast: Option<u32>,
    pub slow: Option<u32>,
    pub lookback: Option<u32>,
    pub skip: Option<u32>,
    pub top_k: u32,
    pub rebalance: String,
}

/// 成本假设参数（十进制字符串原样透传，校验由协调器负责）。
#[derive(Debug, Clone)]
pub struct CostArgs {
    pub commission_rate: String,
    pub min_commission: String,
    pub sell_tax: String,
    pub other_fee: String,
    pub slippage_bps: String,
    pub participation: String,
    pub eff_start: String,
    pub eff_end: String,
}

/// 解析后的子命令调用。
#[derive(Debug, Clone)]
pub enum Invocation {
    Import {
        g: Global,
        source: String,
        paths: Vec<String>,
        symbols: Option<Vec<String>>,
        start: Option<String>,
        end: Option<String>,
        price_basis: String,
        auxiliary_kind: Option<String>,
        wait: bool,
        wait_timeout: u64,
        idem: Option<String>,
    },
    UniversePreview {
        g: Global,
        snapshot: String,
        as_of: String,
        mode: String,
        membership: String,
        /// 内联 JSON 或 `@文件路径`。
        rule: String,
        missing_policy: Option<String>,
        wait: bool,
        wait_timeout: u64,
        idem: Option<String>,
    },
    UniverseSave {
        g: Global,
        preview_task: String,
        preview_hash: String,
        input_hash: String,
        name: String,
    },
    RunSubmit {
        g: Global,
        snapshot: String,
        universe: String,
        strategy: StrategyArgs,
        start: String,
        end: String,
        training_end: Option<String>,
        test_start: Option<String>,
        capital: String,
        costs: CostArgs,
        rules_hash: String,
        mode: String,
        /// `fast=5,10;slow=20,30` 形式；缺省无网格。
        grid: Option<String>,
        seed: u64,
        benchmark: Option<String>,
        wait: bool,
        wait_timeout: u64,
        idem: Option<String>,
    },
    RunCancel {
        g: Global,
        task: String,
    },
    Compare {
        g: Global,
        runs: Vec<String>,
        view: String,
        benchmark: Option<String>,
    },
    PlanGenerate {
        g: Global,
        snapshot: String,
        universe: String,
        as_of: String,
        strategy: StrategyArgs,
        costs: CostArgs,
        rules_hash: String,
        mode: String,
        allow_historical: bool,
        /// 手工持仓 JSON（`@文件` 亦可）；缺省无持仓。
        holdings: Option<String>,
        wait: bool,
        wait_timeout: u64,
        idem: Option<String>,
    },
    PlanExport {
        g: Global,
        plan_id: String,
        destination: String,
        overwrite: bool,
    },
    PlanNote {
        g: Global,
        plan_id: String,
        text: String,
        kind: String,
    },
    Snapshots {
        g: Global,
        limit: Option<u32>,
        cursor: Option<String>,
    },
    TaskGet {
        g: Global,
        task: String,
    },
    TaskWait {
        g: Global,
        task: String,
        timeout: u64,
    },
}

/// 中文用法总览（stderr）。
pub fn usage() -> String {
    let mut s = String::from(
        "research —— A 股研究桌面命令行（协调器薄壳）\n\
         \n\
         用法：research --workspace <工作区路径> [--json] <子命令> [参数]\n\
         \n\
         全局参数：\n\
           --workspace <路径>   工作区目录（支持中文与空格；必填）\n\
           --json               以 JSON 输出结果（缺省为中文表格）\n\
         \n\
         子命令：\n\
           import            导入暂存数据（行情/主档/财务/公司行为/日历/规则）\n\
           universe preview  股票池预览；universe save 保存\n\
           run submit        提交回测运行（可 --grid 参数网格、--wait 等待终态）\n\
           run show|cancel   查询/取消任务\n\
           compare           运行比较（2..5 个）\n\
           plan generate|export|note   生成/导出/备注交易计划\n\
           snapshots         列出快照\n\
           task get|wait     查询/等待任意任务终态\n\
         \n\
         退出码：0 成功；3 业务拒绝（含用法错误）；4 环境错误。\n\
         详细参数：research <子命令> --help\n",
    );
    for help in SUBCOMMAND_HELP {
        s.push_str(&format!("\n{help}\n"));
    }
    s
}

const SUBCOMMAND_HELP: &[&str] = &[
    "research import --source tdx|tickflow|auxiliary --paths <文件,文件,...>\n  [--symbols 代码,代码] [--start 日期] [--end 日期]\n  [--price-basis raw|qfq|hfq] [--auxiliary-kind master|financial|actions|calendar|rules]\n  [--wait] [--wait-timeout 秒] [--idempotency-key 键]",
    "research universe preview --snapshot <ID> --as-of <日期> --mode strict|exploratory\n  --membership dynamic|fixed --rule '<JSON|@文件>' [--missing-policy ignore_condition]\n  [--wait] [--wait-timeout 秒] [--idempotency-key 键]",
    "research universe save --preview-task <ID> --preview-hash <SHA256> --input-hash <SHA256> --name <名称>",
    "research run submit --snapshot <ID> --universe <ID> --template ema|momentum [--version v]\n  [--fast N --slow N | --lookback N --skip N] --top-k N --rebalance daily|weekly|monthly\n  --start <日期> --end <日期> [--training-end 日期 --test-start 日期] --capital <金额>\n  --commission-rate <数> --min-commission <数> --sell-tax <数> --other-fee <数>\n  --slippage-bps <数> --participation <数> --eff-start <日期> --eff-end <日期>\n  --rules-hash <SHA256> --mode strict|exploratory [--grid fast=5,10;slow=20] [--seed N]\n  [--benchmark-snapshot <ID>] [--wait] [--wait-timeout 秒] [--idempotency-key 键]",
    "research run show --task <ID>；research run cancel --task <ID>",
    "research compare --runs <ID1,ID2[,ID3..]> --view full|intersection [--benchmark-snapshot <ID>]",
    "research plan generate --snapshot <ID> --universe <ID> --as-of <日期> --template ...\n  （策略/成本参数同 run submit）--rules-hash <SHA256> --mode strict|exploratory\n  [--allow-historical] [--holdings '<JSON|@文件>'] [--wait] [--wait-timeout 秒] [--idempotency-key 键]",
    "research plan export --plan-id <ID> --destination <路径> [--overwrite]",
    "research plan note --plan-id <ID> --text <内容> --kind 备注|已人工处理|放弃",
    "research snapshots [--limit N] [--cursor <游标>]",
    "research task get --task <ID>；research task wait --task <ID> --timeout <秒>",
];

fn subcommand_usage(cmd: &str) -> String {
    let needle = SUBCOMMAND_HELP
        .iter()
        .find(|h| h.starts_with(&format!("research {cmd} ")))
        .or_else(|| SUBCOMMAND_HELP.iter().find(|h| h.contains(&format!("research {cmd}"))));
    match needle {
        Some(h) => format!("{h}\n"),
        None => usage(),
    }
}

/// 收集到的原始 flag（`--key 值` → flags；单独出现的 `--key` → bools）。
#[derive(Debug, Default)]
struct Raw {
    flags: HashMap<String, String>,
    bools: HashSet<String>,
}

impl Raw {
    fn require(&self, name: &str) -> Result<String, String> {
        self.flags
            .get(name)
            .cloned()
            .ok_or_else(|| format!("缺少必填参数 --{name}"))
    }

    fn opt(&self, name: &str) -> Option<String> {
        self.flags.get(name).cloned()
    }

    fn has(&self, name: &str) -> bool {
        self.bools.contains(name) || self.flags.contains_key(name)
    }

    /// 逗号分割列表参数。
    fn list(&self, name: &str) -> Result<Vec<String>, String> {
        let v = self.require(name)?;
        to_list(&v).ok_or_else(|| format!("--{name} 不能为空列表"))
    }

    fn opt_list(&self, name: &str) -> Result<Option<Vec<String>>, String> {
        match self.opt(name) {
            None => Ok(None),
            Some(v) => Some(to_list(&v).ok_or_else(|| format!("--{name} 不能为空列表"))).transpose(),
        }
    }

    fn parse_u32(&self, name: &str) -> Result<Option<u32>, String> {
        match self.opt(name) {
            None => Ok(None),
            Some(v) => v
                .parse::<u32>()
                .map(Some)
                .map_err(|_| format!("--{name} 不是非负整数：{v}")),
        }
    }

    fn parse_u64(&self, name: &str, default: u64) -> Result<u64, String> {
        match self.opt(name) {
            None => Ok(default),
            Some(v) => v
                .parse::<u64>()
                .map_err(|_| format!("--{name} 不是非负整数：{v}")),
        }
    }

    /// 解析 `JSON` 或 `@文件路径` 形态的参数值。
    fn text_or_file(&self, name: &str) -> Result<String, String> {
        let v = self.require(name)?;
        load_text_or_file(name, &v)
    }

    fn opt_text_or_file(&self, name: &str) -> Result<Option<String>, String> {
        match self.opt(name) {
            None => Ok(None),
            Some(v) => load_text_or_file(name, &v).map(Some),
        }
    }

    fn one_of(&self, name: &str, allowed: &[&str]) -> Result<String, String> {
        let v = self.require(name)?;
        if allowed.contains(&v.as_str()) {
            Ok(v)
        } else {
            Err(format!("--{name} 取值须为 {} 之一：得到 {v}", allowed.join("|")))
        }
    }
}

fn to_list(v: &str) -> Option<Vec<String>> {
    let items: Vec<String> = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if items.is_empty() { None } else { Some(items) }
}

/// `@路径` 读取文件内容；否则原样返回。
fn load_text_or_file(flag: &str, v: &str) -> Result<String, String> {
    if let Some(path) = v.strip_prefix('@') {
        std::fs::read_to_string(path)
            .map_err(|e| format!("--{flag} 读取文件失败（{path}）：{e}"))
    } else {
        Ok(v.to_string())
    }
}

/// 消费 flag 流：`--key 后随非 -- 值` 视为带值 flag，否则为布尔 flag。
fn take_flags(args: &[String]) -> Result<Raw, String> {
    let mut raw = Raw::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if !a.starts_with("--") {
            return Err(format!("未知位置参数：{a}（本命令只接受 --flag 形式）"));
        }
        let key = a.trim_start_matches('-').to_string();
        if key.is_empty() {
            return Err("空 flag".to_string());
        }
        match args.get(i + 1) {
            Some(v) if !v.starts_with("--") => {
                raw.flags.insert(key, v.clone());
                i += 2;
            }
            _ => {
                raw.bools.insert(key);
                i += 1;
            }
        }
    }
    Ok(raw)
}

/// 全局段解析结果：全局参数 + 剩余令牌（子命令名与子命令参数）。
struct GlobalSplit {
    g: Global,
    rest: Vec<String>,
}

/// 解析全局段（--workspace/--json），遇到第一个子命令名停止。
fn split_global(argv: &[String]) -> Result<GlobalSplit, String> {
    let mut workspace: Option<PathBuf> = None;
    let mut json = false;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--workspace" => {
                let v = argv
                    .get(i + 1)
                    .ok_or_else(|| "--workspace 缺少路径".to_string())?;
                workspace = Some(PathBuf::from(v));
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            s if s.starts_with("--") => {
                return Err(format!("未知全局参数：{s}（全局仅支持 --workspace/--json）\n\n{}", usage()));
            }
            _ => {
                return Ok(GlobalSplit {
                    g: Global {
                        workspace: workspace
                            .ok_or_else(|| "缺少必填参数 --workspace <工作区路径>".to_string())?,
                        json,
                    },
                    rest: argv[i..].to_vec(),
                });
            }
        }
    }
    Err(usage())
}

/// 解析策略与成本参数段（run submit / plan generate 共用）。
fn parse_strategy(raw: &Raw) -> Result<StrategyArgs, String> {
    Ok(StrategyArgs {
        template: raw.one_of("template", &["ema", "momentum"])?,
        version: raw.opt("version").unwrap_or_else(|| "1".to_string()),
        fast: raw.parse_u32("fast")?,
        slow: raw.parse_u32("slow")?,
        lookback: raw.parse_u32("lookback")?,
        skip: raw.parse_u32("skip")?,
        top_k: raw
            .parse_u32("top-k")?
            .ok_or_else(|| "缺少必填参数 --top-k".to_string())?,
        rebalance: raw.one_of("rebalance", &["daily", "weekly", "monthly"])?,
    })
}

fn parse_costs(raw: &Raw) -> Result<CostArgs, String> {
    Ok(CostArgs {
        commission_rate: raw.require("commission-rate")?,
        min_commission: raw.require("min-commission")?,
        sell_tax: raw.require("sell-tax")?,
        other_fee: raw.require("other-fee")?,
        slippage_bps: raw.require("slippage-bps")?,
        participation: raw.require("participation")?,
        eff_start: raw.require("eff-start")?,
        eff_end: raw.require("eff-end")?,
    })
}

/// 解析完整命令行（argv[0] 为程序名，跳过；为空则视为无参数）。
pub fn parse(argv: &[String]) -> Result<Invocation, String> {
    let argv = if argv.is_empty() { argv } else { &argv[1..] };
    let split = split_global(argv)?;
    let g = split.g;
    let rest = split.rest;
    if rest.is_empty() {
        return Err(usage());
    }
    let cmd = rest[0].clone();
    let args = rest[1..].to_vec();

    // --help / -h：以错误通道承载用法（帮助不是成功执行，退出码 3）。
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Err(subcommand_usage(&cmd));
    }

    // 带 --wait/--wait-timeout/--idempotency-key 的命令先拆出子子命令名。
    let (sub_cmd, raw, wait, wait_timeout, idem) = match cmd.as_str() {
        "universe" | "run" | "plan" | "task" => {
            let sub = args
                .first()
                .ok_or_else(|| format!("{cmd} 缺少子命令（--help 查看用法）"))?
                .clone();
            let tail = &args[1..];
            let r = take_flags(tail)?;
            let w = r.has("wait");
            let t = r.parse_u64("wait-timeout", 600)?;
            let k = r.opt("idempotency-key");
            (Some(sub), r, w, t, k)
        }
        _ => {
            let r = take_flags(&args)?;
            let w = r.has("wait");
            let t = r.parse_u64("wait-timeout", 600)?;
            let k = r.opt("idempotency-key");
            (None, r, w, t, k)
        }
    };

    let inv = match cmd.as_str() {
        "import" => Invocation::Import {
            g,
            source: raw.one_of("source", &["tdx", "tickflow", "auxiliary"])?,
            paths: raw.list("paths")?,
            symbols: raw.opt_list("symbols")?,
            start: raw.opt("start"),
            end: raw.opt("end"),
            price_basis: match raw.opt("price-basis") {
                None => "raw".to_string(),
                Some(v) if ["raw", "qfq", "hfq"].contains(&v.as_str()) => v,
                Some(v) => {
                    return Err(format!("--price-basis 取值须为 raw|qfq|hfq 之一：得到 {v}"))
                }
            },
            auxiliary_kind: raw.opt("auxiliary-kind"),
            wait,
            wait_timeout,
            idem,
        },
        "universe" => {
            let r = raw;
            match sub_cmd.as_deref().unwrap_or_default() {
                "preview" => Invocation::UniversePreview {
                    g,
                    snapshot: r.require("snapshot")?,
                    as_of: r.require("as-of")?,
                    mode: r.one_of("mode", &["strict", "exploratory"])?,
                    membership: r.one_of("membership", &["dynamic", "fixed"])?,
                    rule: r.text_or_file("rule")?,
                    missing_policy: r.opt("missing-policy"),
                    wait,
                    wait_timeout,
                    idem,
                },
                "save" => Invocation::UniverseSave {
                    g,
                    preview_task: r.require("preview-task")?,
                    preview_hash: r.require("preview-hash")?,
                    input_hash: r.require("input-hash")?,
                    name: r.require("name")?,
                },
                other => return Err(format!("未知 universe 子命令：{other}（支持 preview|save）")),
            }
        }
        "run" => {
            let r = raw;
            match sub_cmd.as_deref().unwrap_or_default() {
                "submit" => Invocation::RunSubmit {
                    g,
                    snapshot: r.require("snapshot")?,
                    universe: r.require("universe")?,
                    strategy: parse_strategy(&r)?,
                    start: r.require("start")?,
                    end: r.require("end")?,
                    training_end: r.opt("training-end"),
                    test_start: r.opt("test-start"),
                    capital: r.require("capital")?,
                    costs: parse_costs(&r)?,
                    rules_hash: r.require("rules-hash")?,
                    mode: r.one_of("mode", &["strict", "exploratory"])?,
                    grid: r.opt("grid"),
                    seed: r.parse_u64("seed", 0)?,
                    benchmark: r.opt("benchmark-snapshot"),
                    wait,
                    wait_timeout,
                    idem,
                },
                "show" => Invocation::TaskGet {
                    g,
                    task: r.require("task")?,
                },
                "cancel" => Invocation::RunCancel {
                    g,
                    task: r.require("task")?,
                },
                other => return Err(format!("未知 run 子命令：{other}（支持 submit|show|cancel）")),
            }
        }
        "compare" => {
            // 数量校验前置到解析层（与协调器同口径）：越界不触达协调器。
            let runs = raw.list("runs")?;
            if !(2..=5).contains(&runs.len()) {
                return Err(format!(
                    "--runs 须为 2..5 个运行 ID（逗号分割）：得到 {} 个",
                    runs.len()
                ));
            }
            Invocation::Compare {
                g,
                runs,
                view: raw.one_of("view", &["full", "intersection"])?,
                benchmark: raw.opt("benchmark-snapshot"),
            }
        }
        "plan" => {
            let r = raw;
            match sub_cmd.as_deref().unwrap_or_default() {
                "generate" => Invocation::PlanGenerate {
                    g,
                    snapshot: r.require("snapshot")?,
                    universe: r.require("universe")?,
                    as_of: r.require("as-of")?,
                    strategy: parse_strategy(&r)?,
                    costs: parse_costs(&r)?,
                    rules_hash: r.require("rules-hash")?,
                    mode: r.one_of("mode", &["strict", "exploratory"])?,
                    allow_historical: r.has("allow-historical"),
                    holdings: r.opt_text_or_file("holdings")?,
                    wait,
                    wait_timeout,
                    idem,
                },
                "export" => Invocation::PlanExport {
                    g,
                    plan_id: r.require("plan-id")?,
                    destination: r.require("destination")?,
                    overwrite: r.has("overwrite"),
                },
                "note" => Invocation::PlanNote {
                    g,
                    plan_id: r.require("plan-id")?,
                    text: r.require("text")?,
                    kind: r.require("kind")?,
                },
                other => return Err(format!("未知 plan 子命令：{other}（支持 generate|export|note）")),
            }
        }
        "snapshots" => Invocation::Snapshots {
            g,
            limit: raw.parse_u32("limit")?,
            cursor: raw.opt("cursor"),
        },
        "task" => {
            let r = raw;
            match sub_cmd.as_deref().unwrap_or_default() {
                "get" => Invocation::TaskGet {
                    g,
                    task: r.require("task")?,
                },
                "wait" => Invocation::TaskWait {
                    g,
                    task: r.require("task")?,
                    timeout: r.parse_u64("timeout", 600)?,
                },
                other => return Err(format!("未知 task 子命令：{other}（支持 get|wait）")),
            }
        }
        "help" => return Err(usage()),
        other => return Err(format!("未知子命令：{other}\n\n{}", usage())),
    };
    Ok(inv)
}
