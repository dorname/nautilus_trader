//! Swarm 桥接层：多 Agent 并发分析（M2），通过 OUP turn/start 派发并行子任务。
//!
//! 架构对齐 core-06 M2：swarm Parallel 接入 S17——需求确认后并发派发
//! 数据可行性/反例检索/约束核对三个分析 sub-agent，聚合结果写回项目状态。
//!
//! CPU 红线：并发上限 MAX_PARALLEL_DISPATCH=3（对齐 octos-swarm MAX_CONTRACTS_PER_DISPATCH=128
//! 但 GUI 场景保守取 3）；无活跃 dispatch 时不轮询。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent_bridge::{AgentBridge, AgentError};
use crate::oup::TurnTerminal;

/// 最大并发派发数（CPU 保护）。
pub const MAX_PARALLEL_DISPATCH: usize = 3;

/// Pipeline 单 stage 超时（秒）：真实 LLM 尾延迟实测可达 150s+（moonshot
/// 拥塞时段 m3 回归触发 300s 超时），Pipeline 串行语义下放宽到 600s 吸收波动。
pub const PIPELINE_STAGE_TIMEOUT_SECS: u64 = 600;

/// Swarm 派发拓扑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwarmTopology {
    /// 并行：所有子任务同时派发。
    Parallel,
    /// 串行：一个接一个。
    Sequential,
    /// 管道：前一个输出作为后一个输入。
    Pipeline,
}

/// 子任务契约（对齐 octos-swarm ContractSpec）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskContract {
    /// 稳定标识符（dispatch 内唯一）。
    pub contract_id: String,
    /// 子任务类型标签。
    pub label: String,
    /// 提示词。
    pub prompt: String,
}

/// 子任务状态（对齐 octos-swarm SubtaskOutcome）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtaskState {
    /// 排队中。
    Queued,
    /// 执行中。
    Running,
    /// 成功。
    Succeeded,
    /// 失败。
    Failed,
    /// 已取消。
    Cancelled,
}

/// 子任务结果。
#[derive(Debug, Clone)]
pub struct SubtaskResult {
    pub contract_id: String,
    pub state: SubtaskState,
    /// 输出文本（成功时）。
    pub output: Option<String>,
    /// 错误信息（失败时）。
    pub error: Option<String>,
    /// 本子任务的 turn ID（客户端生成的 UUID；未派发为空串）。
    pub turn_id: String,
    /// 实际发送给 agent 的最终 prompt（Pipeline 注入后口径，供注入断言）。
    pub final_prompt: String,
}

/// 派发终态归约（纯函数，UT-S17-18 判定矩阵）：
/// - 空集或含 Queued/Running → `None`（不得判终态）；
/// - 含 Failed/Cancelled → `Some(Failed)`（失败不得吞为成功）；
/// - 全 Succeeded → `Some(Completed)`。
pub fn reduce_dispatch_terminal(states: &[SubtaskState]) -> Option<SwarmDispatchState> {
    if states.is_empty() {
        return None;
    }
    if states
        .iter()
        .any(|s| matches!(s, SubtaskState::Queued | SubtaskState::Running))
    {
        return None;
    }
    if states
        .iter()
        .any(|s| matches!(s, SubtaskState::Failed | SubtaskState::Cancelled))
    {
        return Some(SwarmDispatchState::Failed);
    }
    Some(SwarmDispatchState::Completed)
}

/// Swarm 派发状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwarmDispatchState {
    /// 未开始。
    Idle,
    /// 进行中（已派发子任务数，总子任务数）。
    Active { dispatched: usize, total: usize },
    /// 已完成。
    Completed,
    /// 失败（部分或全部）。
    Failed,
}

/// Swarm 桥接：管理多 Agent 并发派发。
pub struct SwarmBridge {
    /// 底层 Agent 桥接（每个 sub-agent 一个 session）。
    agents: Vec<AgentBridge>,
    /// 当前派发状态。
    state: SwarmDispatchState,
    /// 子任务结果。
    results: Vec<SubtaskResult>,
}

/// Swarm 操作结果。
pub type SwarmResult<T> = Result<T, SwarmError>;

/// Swarm 错误。
#[derive(Debug)]
pub enum SwarmError {
    /// Agent 错误。
    Agent(AgentError),
    /// 派发数量超限。
    TooManyContracts { max: usize, actual: usize },
    /// 无活跃派发。
    NoActiveDispatch,
}

impl std::fmt::Display for SwarmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwarmError::Agent(e) => write!(f, "Agent 错误: {e}"),
            SwarmError::TooManyContracts { max, actual } => {
                write!(f, "子任务数 {actual} 超过上限 {max}")
            }
            SwarmError::NoActiveDispatch => write!(f, "无活跃派发"),
        }
    }
}

impl std::error::Error for SwarmError {}

impl From<AgentError> for SwarmError {
    fn from(e: AgentError) -> Self {
        SwarmError::Agent(e)
    }
}

impl SwarmBridge {
    /// 创建空闲 Swarm 桥接。
    pub fn new() -> Self {
        Self {
            agents: Vec::new(),
            state: SwarmDispatchState::Idle,
            results: Vec::new(),
        }
    }

    /// 当前派发状态。
    pub fn state(&self) -> SwarmDispatchState {
        self.state.clone()
    }

    /// 子任务结果。
    pub fn results(&self) -> &[SubtaskResult] {
        &self.results
    }

    /// 是否有活跃派发。
    pub fn is_active(&self) -> bool {
        matches!(self.state, SwarmDispatchState::Active { .. })
    }

    /// 派发并行子任务（Parallel 拓扑）。
    ///
    /// 每个 contract 创建一个独立 AgentBridge（独立 session），
    /// 并发派发 turn/start，不等待完成（异步通知由事件循环处理）。
    pub fn dispatch_parallel(
        &mut self,
        base_config: &crate::agent_bridge::AgentSessionConfig,
        contracts: Vec<SubtaskContract>,
    ) -> SwarmResult<Vec<String>> {
        if contracts.len() > MAX_PARALLEL_DISPATCH {
            return Err(SwarmError::TooManyContracts {
                max: MAX_PARALLEL_DISPATCH,
                actual: contracts.len(),
            });
        }

        let mut turn_ids = Vec::new();
        let total = contracts.len();
        self.results.clear();

        for (i, contract) in contracts.iter().enumerate() {
            let mut agent_config = base_config.clone();
            agent_config.session_id = format!("{}-{}", base_config.session_id, contract.contract_id);
            // 每个子 agent 独立数据目录，避免锁冲突
            agent_config.oup.data_dir = format!("{}-agent-{}", base_config.oup.data_dir, i);
            agent_config.oup.instance_dir = format!("{}-agent-{}", base_config.oup.instance_dir, i);

            // 复制 profile 到子 agent 数据目录
            let profile_dst_dir = std::path::Path::new(&agent_config.oup.data_dir).join("profiles");
            std::fs::create_dir_all(&profile_dst_dir).map_err(|e| {
                SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(e)))
            })?;
            let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
            if profile_src.exists() {
                std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).map_err(|e| {
                    SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(e)))
                })?;
            }

            let mut agent = AgentBridge::new(agent_config);
            agent.connect()?;

            let turn_id = agent.submit_prompt(&contract.prompt)?;
            turn_ids.push(turn_id.clone());

            self.results.push(SubtaskResult {
                contract_id: contract.contract_id.clone(),
                state: SubtaskState::Running,
                output: None,
                error: None,
                turn_id,
                final_prompt: contract.prompt.clone(),
            });

            self.agents.push(agent);
        }

        self.state = SwarmDispatchState::Active {
            dispatched: total,
            total,
        };

        Ok(turn_ids)
    }

    /// 派发管道子任务（Pipeline 拓扑）。
    ///
    /// 串行执行（issue #11 修复）：每个 stage 以 `submit_and_wait` 阻塞等待真实
    /// turn/completed，上游 stage 的 assistant_persisted 产物注入下游 prompt 的
    /// `{pipeline_input}` 占位符（RD-011：原实现 `previous_output` 恒空、两 stage
    /// 并行起跑且注入为空，为缺陷）。
    ///
    /// 任一 stage 失败或无文本产物即终止：下游不再派发并标记 Cancelled，总体 Failed。
    /// 每个 stage 完成后断开其 agent 连接（CPU 红线：同一时间仅一个 stage 活跃）。
    pub fn dispatch_pipeline(
        &mut self,
        base_config: &crate::agent_bridge::AgentSessionConfig,
        contracts: Vec<SubtaskContract>,
    ) -> SwarmResult<Vec<String>> {
        if contracts.len() > MAX_PARALLEL_DISPATCH {
            return Err(SwarmError::TooManyContracts {
                max: MAX_PARALLEL_DISPATCH,
                actual: contracts.len(),
            });
        }

        let mut turn_ids = Vec::new();
        let total = contracts.len();
        self.results.clear();
        self.state = SwarmDispatchState::Active {
            dispatched: 0,
            total,
        };

        let mut previous_output: Option<String> = None;
        let mut failed = false;

        for (i, contract) in contracts.iter().enumerate() {
            if failed {
                // 上游失败：下游不再派发（不得标成功）
                self.results.push(SubtaskResult {
                    contract_id: contract.contract_id.clone(),
                    state: SubtaskState::Cancelled,
                    output: None,
                    error: Some("上游 stage 失败，未派发".into()),
                    turn_id: String::new(),
                    final_prompt: String::new(),
                });
                continue;
            }

            let mut agent_config = base_config.clone();
            agent_config.session_id = format!("{}-{}", base_config.session_id, contract.contract_id);
            agent_config.oup.data_dir = format!("{}-agent-{}", base_config.oup.data_dir, i);
            agent_config.oup.instance_dir = format!("{}-agent-{}", base_config.oup.instance_dir, i);

            // 复制 profile
            let profile_dst_dir = std::path::Path::new(&agent_config.oup.data_dir).join("profiles");
            std::fs::create_dir_all(&profile_dst_dir).map_err(|e| {
                SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(e)))
            })?;
            let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
            if profile_src.exists() {
                std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).map_err(|e| {
                    SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(e)))
                })?;
            }

            let mut agent = AgentBridge::new(agent_config);
            agent.connect()?;

            // Pipeline 注入：上游产物替换占位符（首 stage 无上游，原样下发）
            let prompt = if i > 0 {
                let upstream = previous_output.clone().unwrap_or_default();
                contract.prompt.replace("{pipeline_input}", &upstream)
            } else {
                contract.prompt.clone()
            };

            self.state = SwarmDispatchState::Active {
                dispatched: i + 1,
                total,
            };

            match agent.submit_and_wait(&prompt, PIPELINE_STAGE_TIMEOUT_SECS) {
                Ok((turn_id, outcome)) => {
                    turn_ids.push(turn_id.clone());
                    match outcome.assistant_text {
                        Some(text) if !text.trim().is_empty() => {
                            self.results.push(SubtaskResult {
                                contract_id: contract.contract_id.clone(),
                                state: SubtaskState::Succeeded,
                                output: Some(text.clone()),
                                error: None,
                                turn_id,
                                final_prompt: prompt,
                            });
                            previous_output = Some(text);
                        }
                        _ => {
                            failed = true;
                            self.results.push(SubtaskResult {
                                contract_id: contract.contract_id.clone(),
                                state: SubtaskState::Failed,
                                output: None,
                                error: Some("stage 无文本产物（assistant_persisted 为空）".into()),
                                turn_id,
                                final_prompt: prompt,
                            });
                        }
                    }
                }
                Err(e) => {
                    failed = true;
                    self.results.push(SubtaskResult {
                        contract_id: contract.contract_id.clone(),
                        state: SubtaskState::Failed,
                        output: None,
                        error: Some(e.to_string()),
                        turn_id: String::new(),
                        final_prompt: prompt,
                    });
                }
            }

            // stage 收尾即释放进程，再保留下线 agent 维持索引对齐
            agent.disconnect();
            self.agents.push(agent);
        }

        self.state = if failed {
            SwarmDispatchState::Failed
        } else {
            SwarmDispatchState::Completed
        };

        Ok(turn_ids)
    }

    /// 轮询所有 agent 的子任务终态。
    ///
    /// 终态口径（issue #12 修复）：以 turn 事件流（turn/completed / turn/error）为准，
    /// 经 [`AgentBridge::poll_turn_outcome`] 非阻塞泵取；task/list 快照仅作观测返回，
    /// 不参与终态判定（RD-012：空列表不得冒充成功，失败不得吞为成功）。
    pub fn poll_all(&mut self) -> SwarmResult<Vec<Value>> {
        if !self.is_active() {
            return Err(SwarmError::NoActiveDispatch);
        }

        let mut snapshots = Vec::new();

        for i in 0..self.agents.len() {
            // 只对 Running 子任务泵终态事件（agents 与 results 按下标一一对应）
            if matches!(
                self.results.get(i).map(|r| &r.state),
                Some(SubtaskState::Running)
            ) {
                match self.agents[i].poll_turn_outcome() {
                    Ok(Some(TurnTerminal::Completed(outcome))) => {
                        if let Some(r) = self.results.get_mut(i) {
                            r.state = SubtaskState::Succeeded;
                            r.output = outcome.assistant_text;
                        }
                    }
                    Ok(Some(TurnTerminal::Failed(err))) => {
                        if let Some(r) = self.results.get_mut(i) {
                            r.state = SubtaskState::Failed;
                            r.error = Some(err);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        // 记录错误但继续轮询其他 agent；本子任务保持 Running
                        eprintln!("poll error: {e}");
                    }
                }
            }

            // task/list 快照仅作观测（对齐 m2_05 的返回口径），不参与终态判定
            match self.agents[i].poll_tasks() {
                Ok(tasks) => snapshots.push(tasks),
                Err(e) => eprintln!("poll error: {e}"),
            }
        }

        let states: Vec<SubtaskState> = self.results.iter().map(|r| r.state.clone()).collect();
        if let Some(terminal) = reduce_dispatch_terminal(&states) {
            self.state = terminal;
        }

        Ok(snapshots)
    }

    /// 取消所有活跃子任务。
    pub fn cancel_all(&mut self) -> SwarmResult<()> {
        for agent in &mut self.agents {
            if let Ok(tasks) = agent.poll_tasks() {
                if let Some(arr) = tasks.get("tasks").and_then(|t| t.as_array()) {
                    for task in arr {
                        if let Some(task_id) = task.get("task_id").and_then(|t| t.as_str()) {
                            let _ = agent.cancel_task(task_id);
                        }
                    }
                }
            }
        }

        for result in &mut self.results {
            if result.state == SubtaskState::Running {
                result.state = SubtaskState::Cancelled;
            }
        }

        self.state = SwarmDispatchState::Failed;
        Ok(())
    }

    /// 断开所有 agent 连接。
    pub fn disconnect_all(&mut self) {
        for agent in &mut self.agents {
            agent.disconnect();
        }
        self.agents.clear();
        self.state = SwarmDispatchState::Idle;
    }
}

impl Default for SwarmBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SwarmBridge {
    fn drop(&mut self) {
        self.disconnect_all();
    }
}

/// S19 实验验证契约（单 Agent + 协调器集成）。
pub fn s19_experiment_contracts(version_id: &str, data_snapshot: &str) -> Vec<SubtaskContract> {
    vec![SubtaskContract {
        contract_id: "experiment".into(),
        label: "实验执行".into(),
        prompt: format!(
            "执行冻结版本 {} 的回测实验（数据快照：{}）。\n\n\
            注意：数值结果必须来自协调器计算任务，不得自行生成。\n\
            输出：1) 实验状态 2) 事件轨迹摘要 3) 账户净值验证",
            version_id, data_snapshot
        ),
    }]
}

/// S18 设计→代码检查的 Pipeline 契约。
pub fn s18_design_to_code_contracts(requirement_text: &str) -> Vec<SubtaskContract> {
    vec![
        SubtaskContract {
            contract_id: "design".into(),
            label: "设计生成".into(),
            prompt: format!(
                "根据以下需求生成策略设计说明：{}\n\n输出：1) 设计说明 2) 处理流程图节点 3) 参数定义",
                requirement_text
            ),
        },
        SubtaskContract {
            contract_id: "code-check".into(),
            label: "代码检查".into(),
            prompt: "基于以下设计说明生成策略代码框架并检查：{pipeline_input}\n\n输出：1) 代码框架 2) 静态检查结果 3) 与预置示例比对".into(),
        },
    ]
}

/// S17 需求分析的标准三并发契约。
pub fn s17_requirement_contracts(requirement_text: &str) -> Vec<SubtaskContract> {
    vec![
        SubtaskContract {
            contract_id: "feasibility".into(),
            label: "数据可行性分析".into(),
            prompt: format!(
                "分析以下研究需求的数据可行性：{}\n\n评估：1) 所需数据是否可得 2) 数据质量要求 3) 潜在数据缺口",
                requirement_text
            ),
        },
        SubtaskContract {
            contract_id: "counterexample".into(),
            label: "反例检索".into(),
            prompt: format!(
                "针对以下研究需求检索反例和边界情况：{}\n\n找出：1) 类似研究失败案例 2) 常见陷阱 3) 约束条件冲突",
                requirement_text
            ),
        },
        SubtaskContract {
            contract_id: "constraint".into(),
            label: "约束核对".into(),
            prompt: format!(
                "核对以下研究需求的约束条件：{}\n\n检查：1) 资源约束 2) 时间约束 3) 技术约束 4) 合规约束",
                requirement_text
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-swarm-01：SubtaskContract 序列化。
    #[test]
    fn contract_serializes() {
        let c = SubtaskContract {
            contract_id: "test".into(),
            label: "测试".into(),
            prompt: "prompt".into(),
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"contract_id\":\"test\""));
    }

    /// UT-swarm-02：SwarmBridge 初始状态。
    #[test]
    fn swarm_initial_state() {
        let swarm = SwarmBridge::new();
        assert_eq!(swarm.state(), SwarmDispatchState::Idle);
        assert!(!swarm.is_active());
        assert!(swarm.results().is_empty());
    }

    /// UT-swarm-03：超过并发上限返回错误。
    #[test]
    fn dispatch_too_many_contracts() {
        let mut swarm = SwarmBridge::new();
        let config = crate::agent_bridge::AgentSessionConfig::default();
        let contracts: Vec<_> = (0..10)
            .map(|i| SubtaskContract {
                contract_id: format!("c{i}"),
                label: format!("任务{i}"),
                prompt: format!("prompt {i}"),
            })
            .collect();

        let result = swarm.dispatch_parallel(&config, contracts);
        assert!(matches!(
            result,
            Err(SwarmError::TooManyContracts { max: 3, actual: 10 })
        ));
    }

    /// UT-swarm-04：S17 契约生成。
    #[test]
    fn s17_contracts_generation() {
        let contracts = s17_requirement_contracts("测试需求");
        assert_eq!(contracts.len(), 3);
        assert_eq!(contracts[0].contract_id, "feasibility");
        assert_eq!(contracts[1].contract_id, "counterexample");
        assert_eq!(contracts[2].contract_id, "constraint");
        assert!(contracts[0].prompt.contains("测试需求"));
    }

    /// UT-swarm-05：SwarmError Display。
    #[test]
    fn swarm_error_display() {
        let e = SwarmError::TooManyContracts { max: 3, actual: 5 };
        assert!(format!("{e}").contains("5"));
        assert!(format!("{e}").contains("3"));
    }

    /// UT-swarm-06：无活跃派发时轮询返回错误。
    #[test]
    fn poll_without_dispatch_fails() {
        let mut swarm = SwarmBridge::new();
        let result = swarm.poll_all();
        assert!(matches!(result, Err(SwarmError::NoActiveDispatch)));
    }
}
