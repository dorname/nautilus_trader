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

/// 依据各子任务 interrupt 是否真实发出归约取消结果（纯函数，UT-S17-20）：
/// - 仅 Running 子任务参与归约：`issued=true` → Cancelled；`false` → Failed
///   并写入诚实错误（不伪装取消成功，issue #28）；
/// - 非 Running 子任务保持原状；
/// - 返回中断未发出的子任务数（>0 时调用方必须返回错误）。
pub fn mark_cancel_outcomes(results: &mut [SubtaskResult], issued: &[bool]) -> usize {
    let mut missed = 0;
    for (r, ok) in results.iter_mut().zip(issued.iter()) {
        if r.state != SubtaskState::Running {
            continue;
        }
        if *ok {
            r.state = SubtaskState::Cancelled;
        } else {
            r.state = SubtaskState::Failed;
            r.error = Some("中断请求未发出（不伪装取消成功）".into());
            missed += 1;
        }
    }
    missed
}

/// 聚合校验（core-06 M4.3 validator 语义，issue #23；UT-S17-26）：
/// 仅当全部子任务 Succeeded 且 output 非空白时才允许产物挂载（true）；
/// 其余一律阻断（false）。空结果集不算成功。
pub fn aggregate_valid(results: &[SubtaskResult]) -> bool {
    !results.is_empty()
        && results.iter().all(|r| {
            r.state == SubtaskState::Succeeded
                && r.output.as_deref().is_some_and(|s| !s.trim().is_empty())
        })
}

/// 解析 base 配置的 profile 路径（issue #24：禁止机器绝对路径硬编码）。
/// profile 由 `AgentSessionConfig::desktop_with_profile` 解析复制到 base 数据
/// 目录；缺失时明确报错（不静默跳过导致子 agent 无模型静默失败）。
fn base_profile_path(
    base_config: &crate::agent_bridge::AgentSessionConfig,
) -> SwarmResult<std::path::PathBuf> {
    let p = std::path::Path::new(&base_config.oup.data_dir)
        .join("profiles")
        .join(format!("{}.json", base_config.profile_id));
    if p.exists() {
        Ok(p)
    } else {
        Err(SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "LLM profile 缺失：{}（先经 AgentSessionConfig 解析 profile）",
                    p.display()
                ),
            ),
        ))))
    }
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
    /// 部分子任务中断请求未发出（不得伪装取消成功，issue #28）。
    CancelIncomplete { missed: usize },
}

impl std::fmt::Display for SwarmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwarmError::Agent(e) => write!(f, "Agent 错误: {e}"),
            SwarmError::TooManyContracts { max, actual } => {
                write!(f, "子任务数 {actual} 超过上限 {max}")
            }
            SwarmError::NoActiveDispatch => write!(f, "无活跃派发"),
            SwarmError::CancelIncomplete { missed } => {
                write!(f, "{missed} 个子任务中断请求未发出（不伪装取消成功）")
            }
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

    /// 测试可达：注入一个 Running 子任务（无对应 agent 连接；
    /// UT-S17-20 取消归约「中断发不出」分支用）。
    #[doc(hidden)]
    pub fn test_seed_running(&mut self, contract_id: &str, turn_id: &str) {
        self.results.push(SubtaskResult {
            contract_id: contract_id.into(),
            state: SubtaskState::Running,
            output: None,
            error: None,
            turn_id: turn_id.into(),
            final_prompt: String::new(),
        });
        let total = self.results.len();
        self.state = SwarmDispatchState::Active {
            dispatched: total,
            total,
        };
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
            // issue #24：从 base 配置数据目录取 profile（已解析），缺失即报错
            let profile_src = base_profile_path(base_config)?;
            std::fs::copy(&profile_src, profile_dst_dir.join("octos.json")).map_err(|e| {
                SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(e)))
            })?;

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
            // issue #24：从 base 配置数据目录取 profile（已解析），缺失即报错
            let profile_src = base_profile_path(base_config)?;
            std::fs::copy(&profile_src, profile_dst_dir.join("octos.json")).map_err(|e| {
                SwarmError::Agent(AgentError::Oup(crate::oup::OupError::Io(e)))
            })?;

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
        // issue #19/#28：按 turn_id 对每个 Running 子任务发真实 turn/interrupt；
        // 只有中断请求真实发出的才标 Cancelled，发不出的标 Failed 且整体返回
        // Err——空 tasks 或本地状态机不得伪装成功取消。
        let mut issued = Vec::with_capacity(self.results.len());
        for i in 0..self.results.len() {
            let ok = if self.results[i].state == SubtaskState::Running {
                match self.agents.get_mut(i) {
                    Some(agent) => agent.interrupt_turn().is_ok(),
                    None => false,
                }
            } else {
                true
            };
            issued.push(ok);
        }
        let missed = mark_cancel_outcomes(&mut self.results, &issued);
        let states: Vec<SubtaskState> = self.results.iter().map(|r| r.state.clone()).collect();
        if let Some(s) = reduce_dispatch_terminal(&states) {
            self.state = s;
        }
        if missed > 0 {
            return Err(SwarmError::CancelIncomplete { missed });
        }
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

// ---------------------------------------------------------------- SwarmWorker（GUI 后台线程封装，issue #26）

/// SwarmWorker 事件泵间隔（毫秒）。
const SWARM_WORKER_POLL_INTERVAL_MS: u64 = 500;

/// 拥塞期实测单 turn 尾延迟 270s+，三路并发更慢；预算放宽到 600s。
pub const SWARM_WORKER_TIMEOUT_SECS: u64 = 600;

/// Swarm 进度事件（worker → GUI 帧循环）。
#[derive(Debug, Clone)]
pub struct SwarmEvent {
    /// 派发序号（取消/替换后丢弃过期事件）。
    pub id: u64,
    /// 当前派发状态。
    pub state: SwarmDispatchState,
    /// 子任务快照（三路状态卡数据源）。
    pub results: Vec<SubtaskResult>,
    /// 是否终态事件（此后 worker 回到空闲等待下一派发）。
    pub terminal: bool,
    /// 派发/轮询错误（失败终态时填充）。
    pub error: Option<String>,
}

/// Swarm worker 请求。
enum SwarmRequest {
    /// 派发并行子任务。
    Dispatch {
        id: u64,
        contracts: Vec<SubtaskContract>,
    },
    /// 中断进行中的派发（按任务 ID 对齐；issue #19 真实 interrupt）。
    Cancel {
        id: u64,
    },
}

/// 后台 Swarm worker：独立线程持有 SwarmBridge（含每子 agent 的 octos serve
/// 子进程），GUI 帧循环经 channel 收发，不阻塞渲染。对齐 `AgentWorker` 模式：
/// 同一时间仅一个活跃派发，请求串行处理。
pub struct SwarmWorker {
    tx: Option<std::sync::mpsc::Sender<SwarmRequest>>,
    rx: std::sync::mpsc::Receiver<SwarmEvent>,
    handle: Option<std::thread::JoinHandle<()>>,
    /// 构造时绑定的会话 ID（含项目 ID；项目切换校验用，issue #27）。
    session_id: String,
}

impl SwarmWorker {
    /// 启动 worker 线程（此时不连接 octos；首个 dispatch 才建立子 agent 会话）。
    pub fn spawn(config: crate::agent_bridge::AgentSessionConfig) -> Self {
        let session_id = config.session_id.clone();
        let (req_tx, req_rx) = std::sync::mpsc::channel::<SwarmRequest>();
        let (ev_tx, ev_rx) = std::sync::mpsc::channel::<SwarmEvent>();
        let handle = std::thread::spawn(move || Self::run(config, req_rx, ev_tx));
        Self {
            tx: Some(req_tx),
            rx: ev_rx,
            handle: Some(handle),
            session_id,
        }
    }

    /// 绑定的会话 ID（项目隔离校验用，issue #27）。
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    fn run(
        config: crate::agent_bridge::AgentSessionConfig,
        req_rx: std::sync::mpsc::Receiver<SwarmRequest>,
        ev_tx: std::sync::mpsc::Sender<SwarmEvent>,
    ) {
        while let Ok(req) = req_rx.recv() {
            let SwarmRequest::Dispatch { id, contracts } = req else {
                // 空闲期收到的 Cancel 无作用对象，忽略
                continue;
            };
            let mut swarm = SwarmBridge::new();
            if let Err(e) = swarm.dispatch_parallel(&config, contracts) {
                let _ = ev_tx.send(SwarmEvent {
                    id,
                    state: SwarmDispatchState::Failed,
                    results: Vec::new(),
                    terminal: true,
                    error: Some(e.to_string()),
                });
                continue;
            }
            // core-06 约束 5（issue #23）：派发写 redb 账本；失败降级不阻断
            let ledger_path =
                std::path::Path::new(&config.oup.data_dir).join("swarm-ledger.redb");
            let mut ledger = crate::swarm_ledger::SwarmLedger::open(&ledger_path)
                .map_err(|e| eprintln!("[swarm-ledger] 账本打开失败（派发继续）：{e}"))
                .ok();
            let ledger_id = format!(
                "{}-{id}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0)
            );
            if let Some(l) = &mut ledger {
                let contracts: Vec<String> =
                    swarm.results().iter().map(|r| r.contract_id.clone()).collect();
                let turn_ids: Vec<String> =
                    swarm.results().iter().map(|r| r.turn_id.clone()).collect();
                if let Err(e) = l.record_dispatch(&ledger_id, contracts, turn_ids) {
                    eprintln!("[swarm-ledger] 派发记录失败（派发继续）：{e}");
                }
            }
            // 终态入账助手：Completed→Completed，其余→Failed（不得吞为成功）
            macro_rules! ledger_terminal {
                ($l:expr, $state:expr) => {
                    if let Some(l) = &mut $l {
                        let ls = if $state == SwarmDispatchState::Completed {
                            crate::swarm_ledger::LedgerState::Completed
                        } else {
                            crate::swarm_ledger::LedgerState::Failed
                        };
                        if let Err(e) = l.record_terminal(&ledger_id, ls) {
                            eprintln!("[swarm-ledger] 终态入账失败（忽略）：{e}");
                        }
                    }
                };
            }
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_secs(SWARM_WORKER_TIMEOUT_SECS);
            loop {
                // 忙期请求：对当前派发的 Cancel 发真实 interrupt（issue #19）；
                // 其他 ID 的 Cancel 忽略；忙期新 Dispatch 直接失败回执（单活跃口径）。
                let mut cancelled = false;
                while let Ok(extra) = req_rx.try_recv() {
                    match extra {
                        SwarmRequest::Cancel { id: cid } if cid == id => {
                            let cancel_err = swarm.cancel_all().err().map(|e| e.to_string());
                            ledger_terminal!(ledger, swarm.state());
                            let sent = ev_tx.send(SwarmEvent {
                                id,
                                state: swarm.state(),
                                results: swarm.results().to_vec(),
                                terminal: true,
                                error: cancel_err,
                            });
                            if sent.is_err() {
                                return;
                            }
                            cancelled = true;
                            break;
                        }
                        SwarmRequest::Cancel { .. } => {}
                        SwarmRequest::Dispatch { id: did, .. } => {
                            let _ = ev_tx.send(SwarmEvent {
                                id: did,
                                state: SwarmDispatchState::Failed,
                                results: Vec::new(),
                                terminal: true,
                                error: Some("已有进行中派发，本次请求被拒绝".into()),
                            });
                        }
                    }
                }
                if cancelled {
                    break;
                }
                if let Err(e) = swarm.poll_all() {
                    ledger_terminal!(ledger, SwarmDispatchState::Failed);
                    let _ = ev_tx.send(SwarmEvent {
                        id,
                        state: SwarmDispatchState::Failed,
                        results: swarm.results().to_vec(),
                        terminal: true,
                        error: Some(e.to_string()),
                    });
                    break;
                }
                let terminal = !swarm.is_active();
                if terminal {
                    ledger_terminal!(ledger, swarm.state());
                }
                if ev_tx
                    .send(SwarmEvent {
                        id,
                        state: swarm.state(),
                        results: swarm.results().to_vec(),
                        terminal,
                        error: None,
                    })
                    .is_err()
                {
                    return;
                }
                if terminal {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    // 超时：真实取消并把剩余 Running 置 Cancelled（不得吞为成功）
                    let _ = swarm.cancel_all();
                    ledger_terminal!(ledger, swarm.state());
                    let _ = ev_tx.send(SwarmEvent {
                        id,
                        state: swarm.state(),
                        results: swarm.results().to_vec(),
                        terminal: true,
                        error: Some(format!(
                            "派发超时（{}s 未达终态）",
                            SWARM_WORKER_TIMEOUT_SECS
                        )),
                    });
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(SWARM_WORKER_POLL_INTERVAL_MS));
            }
            swarm.disconnect_all();
        }
    }

    /// 派发并行子任务（非阻塞；进度事件经 `try_recv` 获取）。
    pub fn dispatch(&self, id: u64, contracts: Vec<SubtaskContract>) -> Result<(), String> {
        let tx = self.tx.as_ref().ok_or("swarm worker 已关闭")?;
        tx.send(SwarmRequest::Dispatch { id, contracts })
            .map_err(|e| format!("swarm worker 已退出：{e}"))
    }

    /// 请求中断进行中的派发（非阻塞；worker 对当前任务发真实 turn/interrupt，
    /// 终态事件经 `try_recv` 获取）。空闲期或无匹配任务时静默忽略。
    pub fn cancel(&self, id: u64) -> Result<(), String> {
        let tx = self.tx.as_ref().ok_or("swarm worker 已关闭")?;
        tx.send(SwarmRequest::Cancel { id })
            .map_err(|e| format!("swarm worker 已退出：{e}"))
    }

    /// 非阻塞接收一个进度事件。
    pub fn try_recv(&self) -> Option<SwarmEvent> {
        self.rx.try_recv().ok()
    }
}

impl Drop for SwarmWorker {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 契约 ID → 中文标签（S17 标准契约；未知 ID 原样返回）。
fn contract_label(id: &str) -> &str {
    match id {
        "feasibility" => "数据可行性",
        "counterexample" => "反例检索",
        "constraint" => "约束核对",
        other => other,
    }
}

/// 聚合子任务产物为需求分析摘要（挂到 R_n 的 `swarm_analysis`）。
pub fn summarize_swarm_results(results: &[SubtaskResult]) -> String {
    let mut out = String::new();
    for r in results {
        let body = r.output.as_deref().unwrap_or("（无产物）");
        let snippet: String = body.chars().take(200).collect();
        out.push_str(&format!("【{}】{}\n", contract_label(&r.contract_id), snippet));
    }
    out
}

// ---------------------------------------------------------------- S19 实验参数整理（issue #15）

/// S19 实验参数（LLM 整理输出；只含表单参数，不得含任何结果数值）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunParams {
    pub fast: u32,
    pub slow: u32,
    pub top_k: u32,
    pub start: String,
    pub end: String,
    pub capital: String,
}

/// LLM 输出中禁止出现的结果数值字段（实验数值只来自协调器任务）。
const BANNED_RESULT_FIELDS: &[&str] = &[
    "total_return",
    "return_pct",
    "sharpe",
    "max_drawdown",
    "equity",
    "result",
    "metrics",
];

/// 从 LLM 回复提取 JSON 对象体（容忍 ```json fence 与前后散文）。
fn extract_json_object(reply: &str) -> Option<&str> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    (start <= end).then(|| &reply[start..=end])
}

fn json_u32(obj: &serde_json::Map<String, Value>, key: &str) -> Result<u32, String> {
    let v = obj.get(key).ok_or_else(|| format!("缺少字段 {key}"))?;
    let n = match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
    .ok_or_else(|| format!("{key} 须为正整数"))?;
    u32::try_from(n).map_err(|_| format!("{key} 超出范围"))
}

fn json_text(obj: &serde_json::Map<String, Value>, key: &str) -> Result<String, String> {
    let v = obj.get(key).ok_or_else(|| format!("缺少字段 {key}"))?;
    match v {
        Value::String(s) if !s.trim().is_empty() => Ok(s.trim().to_string()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(format!("{key} 须为非空字符串或数字")),
    }
}

/// 校验 YYYY-MM-DD 日期格式（月 01-12、日 01-31）。
fn validate_date(s: &str) -> Result<(), String> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return Err(format!("日期 {s} 须为 YYYY-MM-DD 格式"));
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(format!("日期 {s} 含非法字符"));
    }
    let month: u32 = s[5..7].parse().map_err(|_| format!("日期 {s} 非法"))?;
    let day: u32 = s[8..10].parse().map_err(|_| format!("日期 {s} 非法"))?;
    if !(1..=12).contains(&month) {
        return Err(format!("日期 {s} 月份越界"));
    }
    if !(1..=31).contains(&day) {
        return Err(format!("日期 {s} 日越界"));
    }
    Ok(())
}

/// 解析 LLM 整理的实验参数（S19 职责边界：LLM 只整理参数，数值只来自协调器）。
///
/// 严格校验：非 JSON / 缺字段 / fast>=slow / top_k=0 / 非正 capital / 非法日期
/// 一律拒绝；输出含结果数值字段（total_return 等）一律拒绝。
pub fn parse_run_params(reply: &str) -> Result<RunParams, String> {
    let body = extract_json_object(reply).ok_or_else(|| "回复中未找到 JSON 对象".to_string())?;
    let v: Value = serde_json::from_str(body).map_err(|e| format!("参数 JSON 解析失败：{e}"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| "参数须为 JSON 对象".to_string())?;
    for key in BANNED_RESULT_FIELDS {
        if obj.contains_key(*key) {
            return Err(format!(
                "LLM 输出不得包含结果数值字段 {key}（实验数值只来自协调器任务）"
            ));
        }
    }
    let fast = json_u32(obj, "fast")?;
    let slow = json_u32(obj, "slow")?;
    if fast == 0 || slow == 0 {
        return Err("fast/slow 须为正整数".into());
    }
    if fast >= slow {
        return Err(format!("fast（{fast}）须小于 slow（{slow}）"));
    }
    let top_k = json_u32(obj, "top_k")?;
    if top_k == 0 {
        return Err("top_k 须为正整数".into());
    }
    let start = json_text(obj, "start")?;
    let end = json_text(obj, "end")?;
    validate_date(&start).map_err(|e| format!("start：{e}"))?;
    validate_date(&end).map_err(|e| format!("end：{e}"))?;
    if start > end {
        return Err(format!("start（{start}）不得晚于 end（{end}）"));
    }
    let capital = json_text(obj, "capital")?;
    let c: f64 = capital.parse().map_err(|_| "capital 须为数字".to_string())?;
    if !c.is_finite() || c <= 0.0 {
        return Err("capital 须为正数".to_string());
    }
    Ok(RunParams {
        fast,
        slow,
        top_k,
        start,
        end,
        capital,
    })
}

/// 参数映射为运行表单（费用等环境字段沿用表单默认值）。
pub fn build_run_form(params: &RunParams) -> crate::bridge::RunForm {
    crate::bridge::RunForm {
        fast: params.fast,
        slow: params.slow,
        top_k: params.top_k,
        start: params.start.clone(),
        end: params.end.clone(),
        capital: params.capital.clone(),
        ..Default::default()
    }
}

/// S19 参数整理 prompt（LLM 只输出 JSON 表单参数，不输出解释与结果指标）。
pub fn s19_organize_params_prompt(requirement_text: &str, design_note: &str) -> String {
    format!(
        "将以下研究需求与策略设计整理为回测参数。\n\n\
        研究需求：{requirement_text}\n\n\
        策略设计：{design_note}\n\n\
        只输出一个 JSON 对象（不要输出任何解释、markdown 标记或结果指标），字段：\n\
        - fast：短均线周期（正整数）\n\
        - slow：长均线周期（正整数，必须大于 fast）\n\
        - top_k：持仓数量（正整数）\n\
        - start：回测开始日期（YYYY-MM-DD，须在数据区间内）\n\
        - end：回测结束日期（YYYY-MM-DD，不早于 start）\n\
        - capital：初始资金 CNY（正数）"
    )
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
