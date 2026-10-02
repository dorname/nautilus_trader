//! AI 工作台状态机（S17～S20）：项目隔离、版本冻结、上游过期、证据边界、计划核对。
//!
//! 纯逻辑（可自动化部分由 research-desktop UT/ST 承载，落地口径见 S17~S20
//! 测试文档「GUI 落地口径」段）：实验执行由 GUI 层经协调器桥完成（仅引用
//! task_id），本模块不触碰协调器；渲染/焦点旅程保持 [manual]。

/// 对话消息角色。
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub text: String,
}

/// 需求版本 R（确认后不可变）。
#[derive(Debug, Clone)]
pub struct RequirementVersion {
    pub id: usize,
    pub text: String,
    pub acceptance: String,
    /// 投入比例（0..=1）。
    pub allocation: f64,
    /// 最低成交额（非负）。
    pub min_amount: f64,
}

/// 设计版本 D（绑定需求版本）。
#[derive(Debug, Clone)]
pub struct DesignVersion {
    pub id: usize,
    /// 绑定的需求版本 ID。
    pub req_id: usize,
    pub note: String,
    pub allocation: f64,
    pub min_amount: f64,
}

/// 代码版本 v（不可变；冻结 R/D 引用与保存时的修订计数）。
#[derive(Debug, Clone)]
pub struct CodeVersion {
    pub id: usize,
    pub source: String,
    /// 冻结的需求版本 ID。
    pub req_id: usize,
    /// 冻结的设计版本 ID。
    pub design_id: usize,
    /// 冻结戳：保存时的项目修订计数（上游过期判定基准）。
    pub stamp: u64,
}

/// 实验 E（冻结版本引用；执行由协调器承载，此处仅留证据）。
#[derive(Debug, Clone)]
pub struct Experiment {
    pub id: usize,
    /// 冻结的代码版本 ID。
    pub version_id: usize,
    /// 运行时的修订计数（过期后历史实验不受影响的对照证据）。
    pub stamp: u64,
    /// 协调器任务 ID（执行引用；取消无产物时为 None）。
    pub task_id: Option<String>,
    /// 期末总收益（比较用；未完成/取消为 None）。
    pub total_return: Option<String>,
}

/// 验证报告（证据边界：演示检查可过，正式验证恒「证据不足」）。
#[derive(Debug, Clone)]
pub struct Report {
    pub version_id: usize,
    pub run_id: usize,
    /// 各项检查（名称, 通过, 说明）；末项「正式策略验证」恒 false。
    pub checks: Vec<(String, bool, String)>,
}

impl Report {
    /// 演示检查是否全部通过（不含恒 false 的正式验证项）。
    pub fn demo_pass(&self) -> bool {
        self.checks
            .iter()
            .filter(|(name, _, _)| name != "正式策略验证")
            .all(|(_, ok, _)| *ok)
    }
}

/// 计划草稿（冻结签名：账户/数据输入变更即过期）。
#[derive(Debug, Clone)]
pub struct PlanDraft {
    pub version_id: usize,
    pub snapshot: String,
    pub trade_date: String,
    pub cash: f64,
    pub total_assets: f64,
    /// 行（代码, 参考价, 当前持有, 可卖, 目标, 调整数量）。
    pub rows: Vec<(String, f64, i64, i64, i64, i64)>,
    /// 冻结签名（生成时的输入指纹）。
    pub signature: u64,
    /// 核对是否通过（确认导出前提）。
    pub checked: bool,
}

/// 项目（会话与版本完全隔离；切回时消息/版本/实验保留）。
#[derive(Debug, Clone)]
pub struct Project {
    pub id: usize,
    pub name: String,
    pub messages: Vec<ChatMessage>,
    pub reqs: Vec<RequirementVersion>,
    pub active_req: Option<usize>,
    pub designs: Vec<DesignVersion>,
    pub active_design: Option<usize>,
    pub versions: Vec<CodeVersion>,
    pub active_version: Option<usize>,
    pub experiments: Vec<Experiment>,
    pub report: Option<Report>,
    pub plan: Option<PlanDraft>,
    /// 上游修订计数（需求确认/设计保存/版本保存时递增；过期判定基准）。
    pub revision: u64,
}

impl Project {
    /// 新项目（带欢迎消息）。
    pub fn new(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            messages: vec![ChatMessage {
                role: Role::Assistant,
                text: "你好，我们从研究目标开始。我会把需求、设计、代码与验证证据整理在同一个项目中。".into(),
            }],
            reqs: Vec::new(),
            active_req: None,
            designs: Vec::new(),
            active_design: None,
            versions: Vec::new(),
            active_version: None,
            experiments: Vec::new(),
            report: None,
            plan: None,
            revision: 0,
        }
    }
}

/// 需求确认校验（S17：空正文/越界拒绝）。
pub fn validate_requirement(
    text: &str,
    acceptance: &str,
    allocation_pct: f64,
    min_amount: f64,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("请填写研究目标".into());
    }
    if acceptance.trim().is_empty() {
        return Err("请填写验收标准".into());
    }
    if !(0.0..=100.0).contains(&allocation_pct) {
        return Err("投入比例须为 0～100%".into());
    }
    if min_amount < 0.0 {
        return Err("成交额须为非负数".into());
    }
    Ok(())
}

/// 工作台：项目集合与活动项目（会话态随应用存活；切页/切项目不丢失）。
#[derive(Debug, Clone)]
pub struct Workspace {
    pub projects: Vec<Project>,
    pub active: usize,
    next_project_id: usize,
}

impl Workspace {
    /// 新工作台（含初始项目）。
    pub fn new() -> Self {
        Self {
            projects: vec![Project::new(1, "A股量价 · 策略研究")],
            active: 0,
            next_project_id: 2,
        }
    }

    /// 活动项目。
    pub fn current(&self) -> &Project {
        &self.projects[self.active]
    }

    /// 活动项目（可变）。
    pub fn current_mut(&mut self) -> &mut Project {
        &mut self.projects[self.active]
    }

    /// 新建项目并切换（S17：项目隔离——会话、需求与版本互不串扰）。
    pub fn add_project(&mut self, name: impl Into<String>) -> usize {
        let id = self.next_project_id;
        self.next_project_id += 1;
        self.projects.push(Project::new(id, name));
        self.active = self.projects.len() - 1;
        id
    }

    /// 切换项目（越界保持不变；返回是否切换）。
    pub fn switch_to(&mut self, index: usize) -> bool {
        if index < self.projects.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    /// 确认需求（S17：校验通过后生成 R 版本；上游修订计数递增）。
    pub fn confirm_requirement(
        &mut self,
        text: &str,
        acceptance: &str,
        allocation_pct: f64,
        min_amount: f64,
    ) -> Result<usize, String> {
        validate_requirement(text, acceptance, allocation_pct, min_amount)?;
        let p = self.current_mut();
        let id = p.reqs.len() + 1;
        p.reqs.push(RequirementVersion {
            id,
            text: text.trim().to_string(),
            acceptance: acceptance.trim().to_string(),
            allocation: allocation_pct / 100.0,
            min_amount,
        });
        p.active_req = Some(id);
        p.revision += 1;
        Ok(id)
    }

    /// 生成设计（S18：绑定当前需求版本；无需求版本则拒绝）。
    pub fn generate_design(&mut self, note: impl Into<String>) -> Result<usize, String> {
        let (req_id, allocation, min_amount) = {
            let p = self.current();
            let req_id = p.active_req.ok_or("请先确认需求文档")?;
            let req = &p.reqs[req_id - 1];
            (req.id, req.allocation, req.min_amount)
        };
        let p = self.current_mut();
        let id = p.designs.len() + 1;
        p.designs.push(DesignVersion {
            id,
            req_id,
            note: note.into(),
            allocation,
            min_amount,
        });
        p.active_design = Some(id);
        p.revision += 1;
        Ok(id)
    }

    /// 保存代码版本（S18：不可变；冻结当前 R/D 引用与修订计数；幂等）。
    pub fn save_version(&mut self, source: impl Into<String>) -> Result<usize, String> {
        let source = source.into();
        let (req_id, design_id) = {
            let p = self.current();
            let req_id = p.active_req.ok_or("请先确认需求")?;
            let design_id = p.active_design.ok_or("请先保存设计")?;
            if p.designs[design_id - 1].req_id != req_id {
                return Err("设计未与当前需求匹配".into());
            }
            (req_id, design_id)
        };
        let p = self.current_mut();
        // 相同源码重复保存返回原版本（幂等，不产生新冻结戳）
        if let Some(v) = p.versions.iter().find(|v| v.source == source) {
            p.active_version = Some(v.id);
            return Ok(v.id);
        }
        let id = p.versions.len() + 1;
        p.revision += 1;
        p.versions.push(CodeVersion {
            id,
            source,
            req_id,
            design_id,
            stamp: p.revision,
        });
        p.active_version = Some(id);
        Ok(id)
    }

    /// 版本是否新鲜（S18：上游更新后旧版本不可新运行，历史实验不变）。
    /// 判定 = 冻结引用仍为当前活动引用，且冻结戳等于当前修订计数。
    pub fn version_fresh(&self, version_id: usize) -> bool {
        let p = self.current();
        let Some(v) = p.versions.get(version_id - 1) else {
            return false;
        };
        let refs_match =
            p.active_req == Some(v.req_id) && p.active_design == Some(v.design_id);
        refs_match && p.revision == v.stamp
    }

    /// 运行实验（S18：过期拒绝；冻结版本与修订戳；task_id 由 GUI 回填）。
    pub fn run_experiment(&mut self, task_id: Option<String>) -> Result<usize, String> {
        let version_id = self
            .current()
            .active_version
            .ok_or("请先保存版本")?;
        if !self.version_fresh(version_id) {
            return Err("版本未保存或上游已过期，请保存当前版本后再运行".into());
        }
        let p = self.current_mut();
        let id = p.experiments.len() + 1;
        p.experiments.push(Experiment {
            id,
            version_id,
            stamp: p.revision,
            task_id,
            total_return: None,
        });
        Ok(id)
    }

    /// 回填任务引用（协调器提交成功后；提交失败的实验保留为无任务证据）。
    pub fn attach_task(&mut self, experiment_id: usize, task_id: String) {
        let p = self.current_mut();
        if let Some(e) = p.experiments.iter_mut().find(|e| e.id == experiment_id) {
            e.task_id = Some(task_id);
        }
    }

    /// 回填实验结果（协调器终态后；历史实验不变）。
    pub fn record_experiment(&mut self, experiment_id: usize, total_return: Option<String>) {
        let p = self.current_mut();
        if let Some(e) = p.experiments.iter_mut().find(|e| e.id == experiment_id) {
            e.total_return = total_return;
        }
    }

    /// 实验比较（S19：同输入分歧归因代码；跨输入列差异不归因代码）。
    /// 同输入 = 版本与修订戳完全一致；否则仅列输入差异。
    pub fn compare_experiments(&self, a: usize, b: usize) -> Result<(bool, Vec<String>), String> {
        let p = self.current();
        let ea = p.experiments.iter().find(|e| e.id == a).ok_or("实验不存在")?;
        let eb = p.experiments.iter().find(|e| e.id == b).ok_or("实验不存在")?;
        let same_input = ea.version_id == eb.version_id && ea.stamp == eb.stamp;
        let mut diffs = Vec::new();
        if !same_input {
            if ea.version_id != eb.version_id {
                diffs.push(format!("代码版本：E{a}=v{}，E{b}=v{}", ea.version_id, eb.version_id));
            }
            if ea.stamp != eb.stamp {
                diffs.push(format!("输入修订：E{a}={}, E{b}={}", ea.stamp, eb.stamp));
            }
        }
        Ok((same_input, diffs))
    }

    /// 生成验证报告（S19：无实验拒绝；正式验证恒「证据不足」）。
    pub fn make_report(&mut self) -> Result<usize, String> {
        let version_id = self.current().active_version.ok_or("当前版本不存在")?;
        if !self.version_fresh(version_id) {
            return Err("当前版本已过期或不存在".into());
        }
        let run_id = self
            .current()
            .experiments
            .iter()
            .filter(|e| e.version_id == version_id)
            .max_by_key(|e| e.id)
            .map(|e| e.id)
            .ok_or("当前版本尚无实验，不能生成验证结论")?;
        let has_result = self
            .current()
            .experiments
            .iter()
            .any(|e| e.id == run_id && (e.task_id.is_some() || e.total_return.is_some()));
        let (req_id, design_id) = {
            let v = &self.current().versions[version_id - 1];
            (v.req_id, v.design_id)
        };
        let fresh = self.version_fresh(version_id);
        let report = Report {
            version_id,
            run_id,
            checks: vec![
                (
                    "版本与冻结输入一致".into(),
                    fresh,
                    format!("R{req_id}/D{design_id}，冻结戳 {}", self.current().revision),
                ),
                (
                    "演示检查（实验已执行）".into(),
                    has_result,
                    if has_result {
                        format!("实验 E{run_id} 已在当前版本上执行")
                    } else {
                        "实验未产出执行证据".into()
                    },
                ),
                (
                    "正式策略验证".into(),
                    false,
                    "证据不足：真实数据、样本外与参数稳健性未运行".into(),
                ),
            ],
        };
        let p = self.current_mut();
        p.report = Some(report);
        Ok(version_id)
    }

    /// 生成计划草稿（S20：账户恒等式校验；冻结签名）。
    #[allow(clippy::too_many_arguments)]
    pub fn make_plan(
        &mut self,
        snapshot: impl Into<String>,
        trade_date: impl Into<String>,
        cash: f64,
        total_assets: f64,
        rows: Vec<(String, f64, i64, i64, i64, i64)>,
    ) -> Result<(), String> {
        let version_id = self.current().active_version.ok_or("当前版本不存在")?;
        if !self.version_fresh(version_id) {
            return Err("策略版本或验证报告已过期".into());
        }
        // S20-11：现金 + 持仓市值 = 总资产（参考快照口径）
        let market_value: f64 = rows.iter().map(|(_, price, cur, _, _, _)| price * *cur as f64).sum();
        if (cash + market_value - total_assets).abs() > 0.01 {
            return Err(format!(
                "账户恒等式不成立：现金 {:.0} + 持仓市值 {:.0} ≠ 总资产 {:.0}",
                cash, market_value, total_assets
            ));
        }
        let snapshot = snapshot.into();
        let trade_date = trade_date.into();
        let signature = plan_signature(version_id, &snapshot, &trade_date, cash, &rows);
        let p = self.current_mut();
        p.plan = Some(PlanDraft {
            version_id,
            snapshot,
            trade_date,
            cash,
            total_assets,
            rows,
            signature,
            checked: false,
        });
        Ok(())
    }

    /// 计划核对（S20：过期、签名、报告匹配、现金、可卖、整手）。
    /// `buys_cost_with_fee` 由 GUI 按目标买入金额与费用参数计算后传入。
    pub fn check_plan(&mut self, buys_cost_with_fee: f64) -> Result<Vec<String>, String> {
        let (version_id, signature, snapshot, trade_date, cash, rows) = {
            let p = self.current();
            let t = p.plan.as_ref().ok_or("尚未生成计划")?;
            (
                t.version_id,
                t.signature,
                t.snapshot.clone(),
                t.trade_date.clone(),
                t.cash,
                t.rows.clone(),
            )
        };
        let mut issues = Vec::new();
        if !self.version_fresh(version_id) {
            issues.push("策略版本或验证报告已过期".into());
        }
        let report_ok = self
            .current()
            .report
            .as_ref()
            .is_some_and(|r| r.version_id == version_id);
        if !report_ok {
            issues.push("验证报告缺失或与策略版本不匹配".into());
        }
        if signature != plan_signature(version_id, &snapshot, &trade_date, cash, &rows) {
            issues.push("账户或数据输入已改变，请重新生成计划".into());
        }
        if buys_cost_with_fee > cash {
            issues.push(format!("可用现金不足：买入合计含费 {buys_cost_with_fee:.0}"));
        }
        for (symbol, _price, _current, sellable, _target, delta) in &rows {
            if *delta < 0 && -*delta > *sellable {
                issues.push(format!("{symbol} 可卖数量不足"));
            }
            if *delta > 0 && *delta % 100 != 0 {
                issues.push(format!("{symbol} 买入数量不是 100 股整数倍"));
            }
        }
        let p = self.current_mut();
        if let Some(t) = &mut p.plan {
            t.checked = issues.is_empty();
        }
        Ok(issues)
    }

    /// 确认导出（S20：仅核对通过且确认期间输入未变；产出含演示标识 CSV）。
    /// `current_signature` 由 GUI 按当前账户/数据输入重算后传入。
    pub fn confirm_export(&self, current_signature: u64) -> Result<String, String> {
        let p = self.current();
        let t = p.plan.as_ref().ok_or("尚未生成计划")?;
        if !t.checked {
            return Err("计划未核对或核对未通过".into());
        }
        if t.signature != current_signature {
            return Err("确认期间输入已改变，已阻止导出".into());
        }
        let mut csv = String::from("# 演示标识：研究演示数据，不构成真实交易指令\n");
        csv.push_str(&format!("# 策略版本：v{}\n", t.version_id));
        csv.push_str(&format!("# 交易日：{}\n", t.trade_date));
        csv.push_str("代码,参考价,调整数量\n");
        for (symbol, price, _cur, _sell, _tgt, delta) in &t.rows {
            if *delta != 0 {
                csv.push_str(&format!("{symbol},{price},{delta}\n"));
            }
        }
        Ok(csv)
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

/// 计划冻结签名（FNV-1a 确定性指纹；输入任一变更即失配）。
/// GUI 导出确认前按当前表单输入重算并传入 `confirm_export` 比对。
pub fn plan_signature(
    version_id: usize,
    snapshot: &str,
    trade_date: &str,
    cash: f64,
    rows: &[(String, f64, i64, i64, i64, i64)],
) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(version_id.to_le_bytes().as_slice());
    mix(snapshot.as_bytes());
    mix(trade_date.as_bytes());
    mix(cash.to_le_bytes().as_slice());
    for r in rows {
        mix(r.0.as_bytes());
        mix(r.1.to_le_bytes().as_slice());
        mix(r.2.to_le_bytes().as_slice());
        mix(r.3.to_le_bytes().as_slice());
        mix(r.4.to_le_bytes().as_slice());
        mix(r.5.to_le_bytes().as_slice());
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建好「R→D→v→实验」完整链的工作台。
    fn chained() -> Workspace {
        let mut w = Workspace::new();
        w.confirm_requirement("量价选股", "夏普>0", 30.0, 1_000_000.0).unwrap();
        w.generate_design("EMA 双均线").unwrap();
        w.save_version("fn strategy() {}").unwrap();
        w
    }

    #[test]
    fn requirement_validation_and_versions() {
        let mut w = Workspace::new();
        // 校验拒绝：空正文 / 越界 / 负成交额
        assert_eq!(
            w.confirm_requirement("  ", "x", 10.0, 0.0).unwrap_err(),
            "请填写研究目标"
        );
        assert_eq!(
            w.confirm_requirement("t", " ", 10.0, 0.0).unwrap_err(),
            "请填写验收标准"
        );
        assert_eq!(
            w.confirm_requirement("t", "a", 101.0, 0.0).unwrap_err(),
            "投入比例须为 0～100%"
        );
        assert_eq!(
            w.confirm_requirement("t", "a", 0.0, -1.0).unwrap_err(),
            "成交额须为非负数"
        );
        // 合法确认生成 R1，比例换算 0..=1
        let r1 = w.confirm_requirement("量价", "夏普>0", 30.0, 100.0).unwrap();
        assert_eq!(r1, 1);
        assert!((w.current().reqs[0].allocation - 0.3).abs() < 1e-9);
        // 项目隔离：新项目看不到 R1
        w.add_project("对照项目");
        assert!(w.current().reqs.is_empty());
        assert!(w.switch_to(0));
        assert_eq!(w.current().reqs.len(), 1);
        // 越界切换保持不变
        assert!(!w.switch_to(9));
        assert_eq!(w.active, 0);
    }

    #[test]
    fn freeze_and_staleness() {
        let mut w = chained();
        let v1 = w.current().active_version.unwrap();
        assert!(w.version_fresh(v1), "刚保存的版本新鲜");
        // 上游更新（确认新需求）→ 旧版本过期，不可新运行
        w.confirm_requirement("量价 v2", "夏普>0.5", 40.0, 100.0).unwrap();
        assert!(!w.version_fresh(v1));
        assert_eq!(
            w.run_experiment(Some("T1".into())).unwrap_err(),
            "版本未保存或上游已过期，请保存当前版本后再运行"
        );
        // 切回旧需求引用仍过期（冻结戳已落后）
        w.current_mut().active_req = Some(1);
        w.current_mut().active_design = Some(1);
        assert!(!w.version_fresh(v1), "冻结戳落后即过期");
        // 历史实验不变：过期前记录的实验保留原值
        w.current_mut().active_req = Some(1);
        w.current_mut().active_design = Some(1);
        // 直接构造过期前实验（revision 回放）：v1.stamp = 3
        w.current_mut().experiments.push(Experiment {
            id: 1,
            version_id: v1,
            stamp: 3,
            task_id: Some("T0".into()),
            total_return: Some("0.0625".into()),
        });
        w.confirm_requirement("量价 v3", "夏普>0.6", 50.0, 100.0).unwrap();
        assert_eq!(w.current().experiments[0].total_return.as_deref(), Some("0.0625"));
        // 幂等保存：恢复 v1 的冻结引用后，同源码返回原版本（不产生新冻结戳）
        w.current_mut().active_req = Some(1);
        w.current_mut().active_design = Some(1);
        let again = w.save_version("fn strategy() {}").unwrap();
        assert_eq!(again, v1);
        assert_eq!(w.current().versions.len(), 1);
        assert_eq!(w.current().versions[0].stamp, 3, "幂等保存不改冻结戳");
    }

    #[test]
    fn report_evidence_boundary() {
        let mut w = chained();
        // 无实验 → 拒绝生成验证结论
        assert_eq!(
            w.make_report().unwrap_err(),
            "当前版本尚无实验，不能生成验证结论"
        );
        // 实验完成后生成报告：演示检查可过，正式验证恒「证据不足」
        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        w.record_experiment(e1, Some("0.0625".into()));
        let vid = w.make_report().unwrap();
        let report = w.current().report.as_ref().unwrap();
        assert_eq!(report.version_id, vid);
        assert!(report.demo_pass(), "演示检查通过");
        let formal = report
            .checks
            .iter()
            .find(|(n, _, _)| n == "正式策略验证")
            .unwrap();
        assert!(!formal.1, "正式验证恒不通过");
        assert!(formal.2.contains("证据不足"));
        // 报告勾选项里演示项与恒不过的正式项都在
        assert!(report.checks.iter().any(|(n, _, _)| n == "演示检查（实验已执行）"));
    }

    #[test]
    fn compare_attribution() {
        let mut w = chained();
        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        w.record_experiment(e1, Some("0.05".into()));
        // 同输入（同版本同戳）实验：分歧归因代码
        let e2 = w.run_experiment(Some("T2".into())).unwrap();
        w.record_experiment(e2, Some("0.07".into()));
        let (same, diffs) = w.compare_experiments(e1, e2).unwrap();
        assert!(same, "同版本同戳为同输入");
        assert!(diffs.is_empty());
        // 上游更新后新版本实验：跨输入 → 列差异，不归因代码
        w.confirm_requirement("量价 v2", "夏普>0.5", 40.0, 100.0).unwrap();
        w.generate_design("EMA 双均线 v2").unwrap();
        w.save_version("fn strategy_v2() {}").unwrap();
        let e3 = w.run_experiment(Some("T3".into())).unwrap();
        let (same, diffs) = w.compare_experiments(e1, e3).unwrap();
        assert!(!same);
        assert!(diffs.iter().any(|d| d.contains("代码版本")));
        assert!(diffs.iter().any(|d| d.contains("输入修订")));
    }

    #[test]
    fn plan_check_and_gated_export() {
        let mut w = chained();
        let e1 = w.run_experiment(Some("T1".into())).unwrap();
        w.record_experiment(e1, Some("0.0625".into()));
        w.make_report().unwrap();
        // 账户恒等式：现金 10000 + 持仓市值（10 × 1000）= 总资产 20000
        let rows = |delta: i64| {
            vec![("SYN-A".to_string(), 10.0, 1000, 1000, 1100, delta)]
        };
        // 恒等式不成立 → 生成即拒绝
        assert!(w.make_plan("SNAP-1", "2024-01-05", 10000.0, 99999.0, rows(100)).is_err());
        w.make_plan("SNAP-1", "2024-01-05", 10000.0, 20000.0, rows(100)).unwrap();
        // 未核对 → 导出拒绝
        let sig = w.current().plan.as_ref().unwrap().signature;
        assert_eq!(
            w.confirm_export(sig).unwrap_err(),
            "计划未核对或核对未通过"
        );
        // 核对：可卖不足（卖出超可卖）+ 非整手（买入）逐项暴露
        let mut bad = w.clone();
        bad.current_mut().plan.as_mut().unwrap().rows = vec![
            ("SYN-A".into(), 10.0, 100, 50, 40, -60),
            ("SYN-B".into(), 9.0, 0, 0, 250, 250),
        ];
        let issues = bad.check_plan(5.0).unwrap();
        assert!(issues.iter().any(|i| i.contains("SYN-A 可卖数量不足")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("SYN-B 买入数量不是 100 股整数倍")));
        // 现金不足
        let issues = w.check_plan(999_999.0).unwrap();
        assert!(issues.iter().any(|i| i.contains("可用现金不足")));
        assert!(!w.current().plan.as_ref().unwrap().checked);
        // 核对通过 → 确认导出 → CSV 含演示标识/版本/交易日/调整数量
        let issues = w.check_plan(1500.0).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
        assert!(w.current().plan.as_ref().unwrap().checked);
        let csv = w.confirm_export(sig).unwrap();
        assert!(csv.contains("演示标识"));
        assert!(csv.contains("v1"));
        assert!(csv.contains("2024-01-05"));
        assert!(csv.contains("SYN-A,10,100"));
        // 确认期间输入改变 → 阻断导出
        assert_eq!(
            w.confirm_export(sig + 1).unwrap_err(),
            "确认期间输入已改变，已阻止导出"
        );
        // 报告缺失/不匹配 → 核对暴露
        let mut no_report = w.clone();
        no_report.current_mut().report = None;
        let issues = no_report.check_plan(1500.0).unwrap();
        assert!(issues.iter().any(|i| i.contains("验证报告缺失")));
    }
}
