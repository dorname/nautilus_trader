//! Agent 桥接层：将 GUI 意图（ai::Intent）映射为 OUP 命令，管理 octos session 生命周期。
//!
//! 架构对齐 core-06 M1：单 Agent 对话通路——对话 → OUP turn/start → octos serve →
//! 结果写回 SQLite 项目状态。
//!
//! CPU 红线：OUP 客户端阻塞等待响应，但仅在用户提交对话时触发；
//! 无活跃 turn 时不占用 CPU。

use serde_json::Value;

use crate::oup::{OupClient, OupConfig, OupError, TurnOutcome, TurnTerminal};

/// 单 turn 默认超时。moonshot 拥塞期实测尾延迟 270s+（2026-10-09 验收窗口），
/// 预算放宽到 480s（embed-octos-runtime：实测 API 尾延迟下 420s 偶发误杀，
/// m3 串行 stage 全套时曾双双超时、单跑压线通过；worker 总预算 600s 内）：
/// 宁可慢等真实终态，不得把慢当作失败（诚实口径不变）。
pub const DEFAULT_TURN_TIMEOUT_SECS: u64 = 480;

/// Agent 会话配置。
#[derive(Debug, Clone)]
pub struct AgentSessionConfig {
    /// 会话 ID（与 research-desktop 项目 ID 绑定）。
    pub session_id: String,
    /// Profile ID（octos 运行时配置）。
    pub profile_id: String,
    /// embed-octos-runtime：profile 解析/复制失败警告（构造仍成功；
    /// connect 前置检查会把它升级为显式错误）。
    pub profile_warning: Option<String>,
    /// OUP 客户端配置。
    pub oup: OupConfig,
}

impl Default for AgentSessionConfig {
    fn default() -> Self {
        Self {
            session_id: "research-default".into(),
            profile_id: "octos".into(),
            profile_warning: None,
            oup: OupConfig::default(),
        }
    }
}

impl AgentSessionConfig {
    /// 桌面 GUI 工作配置：独立数据/实例目录（避免与主 octos 实例锁冲突），
    /// 并从主 octos 数据目录复制 profile（含有效 provider 配置）。
    /// session_id 绑定桌面项目 ID（RD-027 项目隔离）。
    pub fn desktop(base_dir: &std::path::Path, project_id: &str) -> Self {
        let profile_src = std::env::var_os("RESEARCH_OCTOS_PROFILE")
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists())
            .or_else(|| {
                let home = std::env::var_os("HOME")?;
                let p = std::path::PathBuf::from(home).join(".octos/profiles/octos.json");
                p.exists().then_some(p)
            });
        Self::desktop_with_profile(base_dir, project_id, profile_src.as_deref())
    }

    /// 同 [`AgentSessionConfig::desktop`]，profile 来源显式给定（便于测试）。
    ///
    /// embed-octos-runtime：profile 复制失败不再 `let _` 吞错——失败写入
    /// [`AgentSessionConfig::profile_warning`]（构造仍成功，便于空态引导），
    /// [`AgentBridge::connect`] 前置检查 profile 存在，缺失即显式报错。
    pub fn desktop_with_profile(
        base_dir: &std::path::Path,
        project_id: &str,
        profile_src: Option<&std::path::Path>,
    ) -> Self {
        // issue #38（fix-issues-38-39-20261013）：base_dir 一律绝对化——
        // 相对路径经 OupClient::spawn 的 current_dir 会在子进程内二次解析
        // 为嵌套路径（profile store 错位 → session/open -32602）。
        // spawn 侧另有防御性绝对化；此处保证配置本身即绝对路径，
        // 使 profile 复制目标与子进程读取视图一致（UT-S17-32）。
        let base_dir = std::path::PathBuf::from(crate::oup::absolutize_path(
            &base_dir.to_string_lossy(),
        ));
        let base_dir = base_dir.as_path();
        let data_dir = base_dir.join("data");
        let instance_dir = base_dir.join("instance");
        let profile_dst_dir = data_dir.join("profiles");
        let mut profile_warning = None;
        if let Err(e) = std::fs::create_dir_all(&profile_dst_dir) {
            profile_warning = Some(format!("创建 profile 目录失败：{e}"));
        }
        let _ = std::fs::create_dir_all(&instance_dir);
        let profile_dst = profile_dst_dir.join("octos.json");
        if !profile_dst.exists() {
            match profile_src {
                Some(src) => {
                    if let Err(e) = std::fs::copy(src, &profile_dst) {
                        profile_warning =
                            Some(format!("复制 LLM profile 失败（{} → {}）：{e}", src.display(), profile_dst.display()));
                    }
                }
                None => {
                    profile_warning = Some(
                        "未找到 LLM profile：已探测 RESEARCH_OCTOS_PROFILE 与 \
                         $HOME/.octos/profiles/octos.json。请先创建 profile（可参考 \
                         octoscode 引导），或以 RESEARCH_OCTOS_PROFILE 显式指定。"
                            .into(),
                    );
                }
            }
        }
        Self {
            session_id: format!("research-{project_id}"),
            profile_id: "octos".into(),
            profile_warning,
            oup: OupConfig {
                octos_bin: "octos".into(),
                data_dir: data_dir.to_string_lossy().into(),
                instance_dir: instance_dir.to_string_lossy().into(),
                // issue #14：子进程 cwd 显式落在工作区根（原为占位 "."）
                workspace_root: base_dir.to_string_lossy().into(),
                extra_env: Vec::new(),
                // embed-octos-runtime：默认 Workspace-Write 免审批档；
                // Full-Access 仅人类显式决策后由调用方置 true。
                danger_full_access: false,
            },
        }
    }

    /// 本会话的 octos 托管工作区根路径（产物回流读取位，embed-octos-runtime）：
    /// `<instance>/profiles/<profile>/data/users/<session_id>/workspace/`。
    /// 2026-10-11 闭环探针实证：agent 工具写文件落盘于此（非子进程 cwd）。
    pub fn managed_workspace_root(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.oup.instance_dir)
            .join("profiles")
            .join(&self.profile_id)
            .join("data/users")
            .join(&self.session_id)
            .join("workspace")
    }
}

/// profile 档位发现（embed-octos-runtime）：扫描 `data_dir/profiles/*.json`
/// 列出可用模型档 ID（供会话选择；目录不存在/不可读返回空，不 panic）。
pub fn discover_profiles(data_dir: &std::path::Path) -> Vec<String> {
    let dir = data_dir.join("profiles");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let ext_ok = p.extension().is_some_and(|x| x == "json");
            let stem = p.file_stem()?.to_string_lossy().into_owned();
            ext_ok.then_some(stem)
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Agent 会话状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionState {
    /// 未连接。
    Disconnected,
    /// 已连接，空闲。
    Connected,
    /// 活跃 turn 进行中。
    TurnActive,
    /// 错误（保留错误信息用于展示）。
    Error,
}

/// Agent 桥接：持有 OUP 客户端，管理 session 与 turn 生命周期。
pub struct AgentBridge {
    client: Option<OupClient>,
    config: AgentSessionConfig,
    state: AgentSessionState,
    /// 最后一次错误信息（用于 UI 展示）。
    last_error: Option<String>,
    /// 活跃 turn ID（submit_prompt 之后由 poll_turn_outcome 追踪终态）。
    active_turn_id: Option<String>,
    /// 跨轮询累积的 assistant_persisted 文本（可能早于终态通知到达）。
    pending_assistant_text: Option<String>,
}

/// Agent 操作结果。
pub type AgentResult<T> = Result<T, AgentError>;

/// 读取 profile 的 `config.env_vars` 并压平为键值对（issue #14）；
/// 文件缺失或结构不符时返回空表（connect 仍继续，LLM 调用失败由
/// turn 层诚实上报）。
fn read_profile_env_vars(config: &AgentSessionConfig) -> Vec<(String, String)> {
    let path = std::path::Path::new(&config.oup.data_dir)
        .join("profiles")
        .join(format!("{}.json", config.profile_id));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    json.get("config")
        .and_then(|c| c.get("env_vars"))
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

/// Agent 错误。
#[derive(Debug)]
pub enum AgentError {
    /// OUP 通信错误。
    Oup(OupError),
    /// 会话未连接。
    NotConnected,
    /// 会话已存在活跃 turn。
    TurnAlreadyActive,
    /// 无活跃 turn 可中断（interrupt 语义拒绝，不得伪装成功）。
    NoActiveTurn,
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Oup(e) => write!(f, "OUP 错误: {e}"),
            AgentError::NotConnected => write!(f, "Agent 会话未连接"),
            AgentError::TurnAlreadyActive => write!(f, "已有活跃对话进行中"),
            AgentError::NoActiveTurn => write!(f, "无活跃对话可中断"),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<OupError> for AgentError {
    fn from(e: OupError) -> Self {
        AgentError::Oup(e)
    }
}

impl AgentBridge {
    /// 创建未连接的 Agent 桥接。
    pub fn new(config: AgentSessionConfig) -> Self {
        Self {
            client: None,
            config,
            state: AgentSessionState::Disconnected,
            last_error: None,
            active_turn_id: None,
            pending_assistant_text: None,
        }
    }

    /// 连接 octos serve（启动子进程并打开 session）。
    pub fn connect(&mut self) -> AgentResult<()> {
        if self.state != AgentSessionState::Disconnected {
            return Ok(()); // 幂等
        }

        // embed-octos-runtime：profile 前置存在性检查——缺失即显式报错并附
        // 构造期警告（不再起一个无凭据的子进程静默失败）。
        let profile_path = std::path::Path::new(&self.config.oup.data_dir)
            .join("profiles")
            .join(format!("{}.json", self.config.profile_id));
        if !profile_path.exists() {
            let hint = self.config.profile_warning.clone().unwrap_or_else(|| {
                "请先创建 LLM profile（$HOME/.octos/profiles/octos.json），或以 \
                 RESEARCH_OCTOS_PROFILE 显式指定来源。"
                    .into()
            });
            return Err(AgentError::Oup(crate::oup::OupError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("LLM profile 缺失：{}。{hint}", profile_path.display()),
            ))));
        }

        // issue #14：显式把 profile `config.env_vars`（如 LLM API key）注入
        // 子进程环境，不再依赖 octos 是否自行加载 profile；值不进日志/仓库。
        self.config.oup.extra_env = read_profile_env_vars(&self.config);

        let mut client = OupClient::spawn(&self.config.oup)?;
        client.session_open_with_profile(&self.config.session_id, &self.config.profile_id)?;

        self.client = Some(client);
        self.state = AgentSessionState::Connected;
        self.last_error = None;
        Ok(())
    }

    /// 测试可达：子进程 PID（UT-S17-24 经 /proc 校验 cwd 用）。
    #[doc(hidden)]
    pub fn test_child_pid(&self) -> Option<u32> {
        self.client.as_ref().map(|c| c.child_pid())
    }

    /// LLM 配置操作的客户端访问(online-chat-llm-config-20261012 / FR-R22):
    /// 仅 worker 线程在已连接后调用。
    pub(crate) fn client_mut(&mut self) -> Option<&mut OupClient> {
        self.client.as_mut()
    }

    /// 测试可达：connect 解析注入的环境变量键名（不含值，防泄漏）。
    #[doc(hidden)]
    pub fn test_extra_env_keys(&self) -> Vec<String> {
        self.config
            .oup
            .extra_env
            .iter()
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// 断开连接（关闭子进程）。
    pub fn disconnect(&mut self) {
        if let Some(mut client) = self.client.take() {
            client.shutdown();
        }
        self.state = AgentSessionState::Disconnected;
    }

    /// 是否已连接。
    pub fn is_connected(&self) -> bool {
        self.state != AgentSessionState::Disconnected
    }

    /// 当前状态。
    pub fn state(&self) -> &AgentSessionState {
        &self.state
    }

    /// 最后错误信息。
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// 提交对话（turn/start），返回 turn ID。
    /// 阻塞等待 turn 启动确认，不等待完成（异步通知由事件循环处理）。
    pub fn submit_prompt(&mut self, prompt: &str) -> AgentResult<String> {
        if self.state == AgentSessionState::TurnActive {
            return Err(AgentError::TurnAlreadyActive);
        }

        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;

        let result = client.turn_start(&self.config.session_id, prompt)?;

        // 从响应中提取 turn_id
        let turn_id = result
            .get("turn_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        self.active_turn_id = Some(turn_id.clone());
        self.pending_assistant_text = None;
        self.state = AgentSessionState::TurnActive;
        Ok(turn_id)
    }

    /// 非阻塞轮询活跃 turn 的终态（issue #12 修复：以 turn/completed / turn/error 为准）。
    ///
    /// - 无活跃 turn 时返回 `Ok(None)`；
    /// - 终态到达后状态回落 `Connected`，并清除 turn 追踪字段；
    /// - `assistant_persisted` 文本跨调用累积，终态时随 [`TurnTerminal::Completed`] 带出。
    pub fn poll_turn_outcome(&mut self) -> AgentResult<Option<TurnTerminal>> {
        if self.state != AgentSessionState::TurnActive {
            return Ok(None);
        }
        let turn_id = match self.active_turn_id.clone() {
            Some(id) => id,
            None => return Ok(None),
        };
        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;

        let mut text = self.pending_assistant_text.take();
        let terminal = client.poll_turn_once(&self.config.session_id, &turn_id, &mut text)?;

        match terminal {
            Some(t) => {
                self.state = AgentSessionState::Connected;
                self.active_turn_id = None;
                self.pending_assistant_text = None;
                Ok(Some(t))
            }
            None => {
                self.pending_assistant_text = text;
                Ok(None)
            }
        }
    }

    /// 收割工具执行事件（embed-octos-runtime）：透传 OUP 客户端积压的
    /// `tool/started`/`tool/completed` 结构化事件，供任务条呈现工具行。
    pub fn take_tool_events(&mut self) -> Vec<crate::oup::ToolEvent> {
        self.client
            .as_mut()
            .map(OupClient::take_tool_events)
            .unwrap_or_default()
    }

    /// 提交对话并等待完成（真实 LLM 调用）。
    ///
    /// 返回 (turn_id, TurnOutcome)。阻塞直到 turn/completed 或超时；
    /// 回复文本经 assistant_persisted 持久化产物提取（RD-018）。
    /// CPU 红线：仅在用户主动提交时调用，不用于轮询。
    pub fn submit_and_wait(&mut self, prompt: &str, timeout_secs: u64) -> AgentResult<(String, TurnOutcome)> {
        if self.state == AgentSessionState::TurnActive {
            return Err(AgentError::TurnAlreadyActive);
        }

        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;

        // 客户端生成 turn_id，并随 turn_start 返回值带回（服务端 accepted 未必含该字段）
        let start_result = client.turn_start(&self.config.session_id, prompt)?;
        let turn_id = start_result
            .get("turn_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::Oup(crate::oup::OupError::Rpc(-32603, "missing turn_id".into())))?
            .to_string();

        self.state = AgentSessionState::TurnActive;
        self.active_turn_id = Some(turn_id.clone());
        self.pending_assistant_text = None;

        // 等待完成（同时泵 rx + notification_rx，避免 turn/completed 饿死）
        let outcome = match client.turn_wait_completed(&self.config.session_id, &turn_id, timeout_secs)
        {
            Ok(v) => v,
            Err(e) => {
                self.state = AgentSessionState::Connected;
                self.active_turn_id = None;
                self.pending_assistant_text = None;
                return Err(AgentError::Oup(e));
            }
        };

        self.state = AgentSessionState::Connected;
        self.active_turn_id = None;
        self.pending_assistant_text = None;
        Ok((turn_id, outcome))
    }

    /// 轮询任务列表（task/list），用于 UI 任务卡片状态更新。
    pub fn poll_tasks(&mut self) -> AgentResult<Value> {
        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;
        Ok(client.task_list(&self.config.session_id)?)
    }

    /// 取消任务（task/cancel）。
    pub fn cancel_task(&mut self, task_id: &str) -> AgentResult<()> {
        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;
        client.task_cancel(&self.config.session_id, task_id)?;
        Ok(())
    }

    /// 中断当前活跃 turn（OUP turn/interrupt，issue #19/#29）。
    ///
    /// 按 `active_turn_id` 向 octos 发真实中断；返回服务端响应
    /// （`{interrupted:true}`）。中断后 turn 以 `turn/error(interrupted)`
    /// 收尾，由后续 `poll_turn_outcome` 归约为 `TurnTerminal::Failed`。
    /// 无活跃 turn 或未连接时明确拒绝（`NoActiveTurn`/`NotConnected`），
    /// 不伪造中断成功（issue #28 口径）。
    pub fn interrupt_turn(&mut self) -> AgentResult<Value> {
        if self.state != AgentSessionState::TurnActive {
            return Err(AgentError::NoActiveTurn);
        }
        let turn_id = self.active_turn_id.clone().ok_or(AgentError::NoActiveTurn)?;
        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;
        Ok(client.turn_interrupt(&self.config.session_id, &turn_id)?)
    }

    /// 列出 agents（agent/list）。
    pub fn list_agents(&mut self) -> AgentResult<Value> {
        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;
        Ok(client.agent_list(&self.config.session_id)?)
    }

    /// 标记 turn 完成（由事件循环或超时调用）。
    pub fn complete_turn(&mut self) {
        if self.state == AgentSessionState::TurnActive {
            self.state = AgentSessionState::Connected;
            self.active_turn_id = None;
            self.pending_assistant_text = None;
        }
    }

    /// 记录错误。
    pub fn record_error(&mut self, msg: impl Into<String>) {
        self.last_error = Some(msg.into());
        self.state = AgentSessionState::Error;
    }
}

impl Drop for AgentBridge {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// 惰性连接保障（Submit 与 LlmOp 共用）：未连接时建立会话，
/// 失败回中文错误文本（调用方回传给 GUI）。
fn ensure_connected(
    config: &AgentSessionConfig,
    bridge: &mut Option<AgentBridge>,
) -> Result<(), String> {
    if bridge.is_none() {
        let mut b = AgentBridge::new(config.clone());
        b.connect()
            .map_err(|e| format!("连接 octos 失败：{e}"))?;
        *bridge = Some(b);
    }
    Ok(())
}

/// Worker 单 turn 结果（GUI 展示口径）。
#[derive(Debug, Clone)]
pub struct WorkerTurn {    /// 合法 UUID turn_id。
    pub turn_id: String,
    /// assistant_persisted 回复文本（模型未产出文本时为 None）。
    pub reply: Option<String>,
    /// 输入 token 数（turn/completed 口径）。
    pub tokens_in: Option<u64>,
    /// 输出 token 数。
    pub tokens_out: Option<u64>,
    /// 本 turn 的工具执行轨迹（embed-octos-runtime：tool/started|completed
    /// 结构化事件按到达序累积，GUI 任务卡呈现 agent 工具执行行）。
    pub tool_trace: Vec<crate::oup::ToolEvent>,
}

/// LLM 配置操作（online-chat-llm-config-20261012 / FR-R22）：
/// 经 worker 线程复用已连接的 OupClient 执行（单活跃口径天然）。
#[derive(Debug, Clone)]
pub enum LlmOp {
    /// 拉取 provider/模型目录。
    Catalog,
    /// 已配置列表（primary + fallbacks）。
    List,
    /// 写入/更新 provider；api_key 仅内存传输。
    Upsert {
        selection: crate::oup::LlmSelectionInput,
        api_key: Option<String>,
        set_primary: bool,
    },
    /// 切换主模型。
    Select { family_id: String, model_id: String },
    /// 连通测试（不写 profile）。
    Test {
        selection: crate::oup::LlmSelectionInput,
        api_key: Option<String>,
    },
}

/// Worker 结果载荷。
#[derive(Debug)]
pub enum WorkerResult {
    /// 对话/生成 turn 结果。
    Turn(WorkerTurn),
    /// LLM 目录。
    Catalog(Vec<crate::oup::LlmFamilyEntry>),
    /// 已配置列表。
    List(crate::oup::LlmListView),
    /// upsert/select/test 的原始 RPC result。
    LlmRpc(serde_json::Value),
}

/// Worker 响应：任务 ID + 结果（Err 为可展示错误文本）。
#[derive(Debug)]
pub struct WorkerResponse {
    /// 提交时分配的任务序号（过期结果由调用方比对丢弃）。
    pub id: u64,
    /// turn/LLM 操作结果或错误文本。
    pub result: Result<WorkerResult, String>,
}

/// Worker 请求。
#[derive(Debug)]
enum WorkerRequest {
    /// 提交 prompt（串行执行）。
    Submit { id: u64, prompt: String },
    /// 中断进行中的 turn（按任务 ID 对齐；issue #19 真实 turn/interrupt）。
    Cancel { id: u64 },
    /// LLM 配置操作（FR-R22；与 Submit 串行同通道）。
    LlmOp { id: u64, op: LlmOp },
}

/// 后台 Agent worker：独立线程持有 AgentBridge（含 octos serve 子进程），
/// GUI 帧循环经 channel 收发，不阻塞渲染。octos 惰性连接：首个任务到来才 spawn。
pub struct AgentWorker {
    tx: Option<std::sync::mpsc::Sender<WorkerRequest>>,
    rx: std::sync::mpsc::Receiver<WorkerResponse>,
    handle: Option<std::thread::JoinHandle<()>>,
    /// 构造时绑定的会话 ID（含项目 ID；项目切换校验用，issue #27）。
    session_id: String,
}

impl AgentWorker {
    /// 启动 worker 线程（此时不连接 octos；首个 submit 才建立会话）。
    pub fn spawn(config: AgentSessionConfig) -> Self {
        let session_id = config.session_id.clone();
        let (req_tx, req_rx) = std::sync::mpsc::channel::<WorkerRequest>();
        let (resp_tx, resp_rx) = std::sync::mpsc::channel::<WorkerResponse>();
        let handle = std::thread::spawn(move || Self::run(config, req_rx, resp_tx));
        Self {
            tx: Some(req_tx),
            rx: resp_rx,
            handle: Some(handle),
            session_id,
        }
    }

    /// 绑定的会话 ID（项目隔离校验用，issue #27）。
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    fn run(
        config: AgentSessionConfig,
        req_rx: std::sync::mpsc::Receiver<WorkerRequest>,
        resp_tx: std::sync::mpsc::Sender<WorkerResponse>,
    ) {
        let mut bridge: Option<AgentBridge> = None;
        while let Ok(req) = req_rx.recv() {
            match req {
                WorkerRequest::Cancel { .. } => {
                    // 空闲期收到的 Cancel 无作用对象，忽略
                    continue;
                }
                WorkerRequest::Submit { id, prompt } => {
                    if let Err(e) = ensure_connected(&config, &mut bridge) {
                        let _ = resp_tx.send(WorkerResponse { id, result: Err(e) });
                        continue;
                    }
                    let b = bridge.as_mut().expect("bridge 已在上方建立");
                    let result = Self::run_turn(b, id, &prompt, &req_rx, &resp_tx)
                        .map(WorkerResult::Turn);
                    if resp_tx.send(WorkerResponse { id, result }).is_err() {
                        return;
                    }
                }
                WorkerRequest::LlmOp { id, op } => {
                    if let Err(e) = ensure_connected(&config, &mut bridge) {
                        let _ = resp_tx.send(WorkerResponse { id, result: Err(e) });
                        continue;
                    }
                    let b = bridge.as_mut().expect("bridge 已在上方建立");
                    let result = Self::run_llm_op(b, op);
                    if resp_tx.send(WorkerResponse { id, result }).is_err() {
                        return;
                    }
                }
            }
        }
    }

    /// 执行 LLM 配置操作（FR-R22）：复用已连接 client，与 turn 串行。
    fn run_llm_op(b: &mut AgentBridge, op: LlmOp) -> Result<WorkerResult, String> {
        let client = b
            .client_mut()
            .expect("ensure_connected 后 client 必存在");
        match op {
            LlmOp::Catalog => client
                .llm_catalog()
                .map(WorkerResult::Catalog)
                .map_err(|e| e.to_string()),
            LlmOp::List => client
                .llm_list()
                .map(WorkerResult::List)
                .map_err(|e| e.to_string()),
            LlmOp::Upsert {
                selection,
                api_key,
                set_primary,
            } => client
                .llm_upsert(&selection, api_key.as_deref(), set_primary)
                .map(WorkerResult::LlmRpc)
                .map_err(|e| e.to_string()),
            LlmOp::Select { family_id, model_id } => client
                .llm_select(&family_id, &model_id)
                .map(WorkerResult::LlmRpc)
                .map_err(|e| e.to_string()),
            LlmOp::Test { selection, api_key } => client
                .llm_test(&selection, api_key.as_deref())
                .map(WorkerResult::LlmRpc)
                .map_err(|e| e.to_string()),
        }
    }

    /// 提交 LLM 配置操作（FR-R22；返回 false = worker 已停）。
    pub fn submit_llm_op(&self, id: u64, op: LlmOp) -> bool {
        self.tx
            .as_ref()
            .is_some_and(|tx| tx.send(WorkerRequest::LlmOp { id, op }).is_ok())
    }

    /// 执行单个 turn：非阻塞提交后轮询终态；轮询间隙处理 Cancel
    /// （真实 turn/interrupt）与忙期新 Submit（回绝，保持单活跃口径）。
    fn run_turn(
        b: &mut AgentBridge,
        id: u64,
        prompt: &str,
        req_rx: &std::sync::mpsc::Receiver<WorkerRequest>,
        resp_tx: &std::sync::mpsc::Sender<WorkerResponse>,
    ) -> Result<WorkerTurn, String> {
        let turn_id = b.submit_prompt(prompt).map_err(|e| e.to_string())?;
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(DEFAULT_TURN_TIMEOUT_SECS);
        let mut interrupted = false;
        let mut tool_trace = Vec::new();
        loop {
            while let Ok(extra) = req_rx.try_recv() {
                match extra {
                    WorkerRequest::Cancel { id: cid } if cid == id => {
                        // 中断发不出时不伪装：标记失败语义由终态归约表达
                        interrupted = b.interrupt_turn().is_ok();
                    }
                    WorkerRequest::Cancel { .. } => {}
                    WorkerRequest::Submit { id: sid, .. } => {
                        let _ = resp_tx.send(WorkerResponse {
                            id: sid,
                            result: Err("另一任务进行中，本次提交被拒绝".into()),
                        });
                    }
                    WorkerRequest::LlmOp { id: lid, .. } => {
                        let _ = resp_tx.send(WorkerResponse {
                            id: lid,
                            result: Err("对话进行中，请稍后再操作模型设置".into()),
                        });
                    }
                }
            }
            match b.poll_turn_outcome() {
                Ok(Some(TurnTerminal::Completed(out))) => {
                    tool_trace.extend(b.take_tool_events());
                    return Ok(WorkerTurn {
                        turn_id,
                        reply: out.assistant_text,
                        tokens_in: out.completed.get("tokens_in").and_then(|v| v.as_u64()),
                        tokens_out: out.completed.get("tokens_out").and_then(|v| v.as_u64()),
                        tool_trace,
                    });
                }
                Ok(Some(TurnTerminal::Failed(msg))) => {
                    return Err(if interrupted {
                        format!("任务已中断（{msg}）")
                    } else {
                        msg
                    });
                }
                Ok(None) => {
                    // embed-octos-runtime：turn 进行中间隙收割工具事件
                    tool_trace.extend(b.take_tool_events());
                }
                Err(e) => return Err(e.to_string()),
            }
            if std::time::Instant::now() >= deadline {
                // 超时：尝试真实中断（不伪造成功），返回超时错误
                let _ = b.interrupt_turn();
                return Err(format!(
                    "turn 超时（{}s 未达终态）",
                    DEFAULT_TURN_TIMEOUT_SECS
                ));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// 提交任务（非阻塞；worker 串行执行，真实 LLM 调用带 120s 超时）。
    pub fn submit(&self, id: u64, prompt: String) -> Result<(), String> {
        let tx = self.tx.as_ref().ok_or("agent worker 已关闭")?;
        tx.send(WorkerRequest::Submit { id, prompt })
            .map_err(|e| format!("agent worker 已退出：{e}"))
    }

    /// 请求中断进行中的任务（非阻塞；worker 对当前 turn 发真实
    /// turn/interrupt，结果仍经 `try_recv` 返回）。空闲期静默忽略。
    pub fn cancel(&self, id: u64) -> Result<(), String> {
        let tx = self.tx.as_ref().ok_or("agent worker 已关闭")?;
        tx.send(WorkerRequest::Cancel { id })
            .map_err(|e| format!("agent worker 已退出：{e}"))
    }

    /// 非阻塞收取已完成结果。
    pub fn try_recv(&self) -> Option<WorkerResponse> {
        self.rx.try_recv().ok()
    }
}

impl Drop for AgentWorker {
    fn drop(&mut self) {
        // 断开请求通道让线程退出 recv 循环（线程 drop AgentBridge 时杀掉 octos 子进程）。
        // 进行中的 turn 最多阻塞到 LLM 超时（120s）后线程才收尾。
        drop(self.tx.take());
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-agent-01：AgentSessionConfig 默认值。
    #[test]
    fn session_config_default() {
        let cfg = AgentSessionConfig::default();
        assert_eq!(cfg.session_id, "research-default");
        assert!(!cfg.oup.octos_bin.is_empty());
    }

    /// UT-agent-02：AgentBridge 初始状态为 Disconnected。
    #[test]
    fn bridge_initial_state() {
        let bridge = AgentBridge::new(AgentSessionConfig::default());
        assert_eq!(bridge.state(), &AgentSessionState::Disconnected);
        assert!(!bridge.is_connected());
        assert!(bridge.last_error().is_none());
    }

    /// UT-agent-03：未连接时提交对话返回 NotConnected。
    #[test]
    fn submit_without_connect_fails() {
        let mut bridge = AgentBridge::new(AgentSessionConfig::default());
        let result = bridge.submit_prompt("test");
        assert!(matches!(result, Err(AgentError::NotConnected)));
    }

    /// UT-agent-04：未连接时轮询任务返回 NotConnected。
    #[test]
    fn poll_without_connect_fails() {
        let mut bridge = AgentBridge::new(AgentSessionConfig::default());
        let result = bridge.poll_tasks();
        assert!(matches!(result, Err(AgentError::NotConnected)));
    }

    /// UT-agent-05：AgentError Display。
    #[test]
    fn agent_error_display() {
        let e = AgentError::NotConnected;
        assert!(format!("{e}").contains("未连接"));
        let e = AgentError::TurnAlreadyActive;
        assert!(format!("{e}").contains("活跃对话"));
    }

    /// UT-agent-06：complete_turn 状态转换。
    #[test]
    fn complete_turn_transitions() {
        let mut bridge = AgentBridge::new(AgentSessionConfig::default());
        // 模拟连接后状态
        bridge.state = AgentSessionState::TurnActive;
        bridge.complete_turn();
        assert_eq!(bridge.state(), &AgentSessionState::Connected);
        // 非 TurnActive 状态不受影响
        bridge.complete_turn();
        assert_eq!(bridge.state(), &AgentSessionState::Connected);
    }

    /// UT-agent-07：record_error 设置错误状态。
    #[test]
    fn record_error_sets_state() {
        let mut bridge = AgentBridge::new(AgentSessionConfig::default());
        bridge.record_error("test error");
        assert_eq!(bridge.state(), &AgentSessionState::Error);
        assert_eq!(bridge.last_error(), Some("test error"));
    }

    /// UT-agent-08：desktop 配置绑定项目 ID 并复制 profile（RD-027）。
    #[test]
    fn desktop_config_binds_project_and_copies_profile() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("rd-agent-cfg-{}-{nanos}", std::process::id()));
        let profile_src = base.join("src-profile.json");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(&profile_src, "{\"providers\":{}}").unwrap();

        let cfg = AgentSessionConfig::desktop_with_profile(&base.join("octos"), "P1", Some(&profile_src));
        assert_eq!(cfg.session_id, "research-P1");
        assert_eq!(cfg.profile_id, "octos");
        let copied = base.join("octos/data/profiles/octos.json");
        assert!(copied.exists(), "profile 已复制到隔离数据目录");
        assert_eq!(std::fs::read_to_string(&copied).unwrap(), "{\"providers\":{}}");
        assert!(base.join("octos/instance").is_dir());

        // 已存在 profile 时不覆盖（保留用户后续修改）
        std::fs::write(&copied, "{\"providers\":{\"x\":1}}").unwrap();
        let _ = AgentSessionConfig::desktop_with_profile(&base.join("octos"), "P1", Some(&profile_src));
        assert_eq!(std::fs::read_to_string(&copied).unwrap(), "{\"providers\":{\"x\":1}}");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// UT-S17-32（fix-issues-38-39-20261013 / issue #38）：相对路径 base_dir
    /// 构造 → 配置各路径全部绝对化（canonicalize），profile 复制目标与
    /// 配置 data_dir 视图一致——堵「测试全用绝对路径」的盲区。
    #[test]
    fn ut_s17_32_relative_base_dir_canonicalized() {
        nautilus_research_testkit::case("UT-S17-32", || {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            // 相对路径（不改进程 cwd，避免并行测试竞态）：以 target/ 下唯一
            // 相对目录模拟 desktop 默认 `research-workspace/octos-agent` 形态
            let rel = format!("target/ut-s17-32-{}-{nanos}", std::process::id());
            let rel_path = std::path::Path::new(&rel);
            assert!(rel_path.is_relative(), "前提：输入为相对路径");

            let cfg = AgentSessionConfig::desktop_with_profile(rel_path, "P1", None);
            assert!(
                std::path::Path::new(&cfg.oup.data_dir).is_absolute(),
                "data_dir 应绝对化，实际：{}",
                cfg.oup.data_dir
            );
            assert!(
                std::path::Path::new(&cfg.oup.instance_dir).is_absolute(),
                "instance_dir 应绝对化"
            );
            assert!(
                std::path::Path::new(&cfg.oup.workspace_root).is_absolute(),
                "workspace_root 应绝对化"
            );
            // 父子视图一致：配置 workspace_root 即 canonicalize 后的真实目录
            let root = std::path::PathBuf::from(&cfg.oup.workspace_root);
            assert!(root.is_dir(), "workspace_root 应已创建");
            assert_eq!(
                root.canonicalize().expect("canonicalize"),
                std::path::PathBuf::from(&rel).canonicalize().expect("canonicalize rel"),
                "配置路径与相对输入指向同一目录"
            );

            let _ = std::fs::remove_dir_all(&root);
        });
    }

    /// UT-agent-09：无 profile 来源时配置仍可用（连接阶段才报诚实错误）。
    #[test]
    fn desktop_config_without_profile() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("rd-agent-cfg2-{}-{nanos}", std::process::id()));
        let cfg = AgentSessionConfig::desktop_with_profile(&base, "P2", None);
        assert_eq!(cfg.session_id, "research-P2");
        assert!(!base.join("data/profiles/octos.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
    /// UT-S17-29（embed-octos-runtime）：profile 档位发现——列出 data_dir
    /// profiles/*.json 的档位 ID；空/缺失目录返回空不 panic。
    #[test]
    fn discover_profiles_lists_ids() {
        nautilus_research_testkit::case("UT-S17-29", || {
            let base = std::env::temp_dir().join(format!(
                "rd-profiles-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let dir = base.join("data");
            std::fs::create_dir_all(dir.join("profiles")).expect("建目录");
            std::fs::write(dir.join("profiles/octos.json"), "{}").expect("写档");
            std::fs::write(dir.join("profiles/kimi.json"), "{}").expect("写档");
            std::fs::write(dir.join("profiles/readme.txt"), "x").expect("写非档");
            let ids = discover_profiles(&dir);
            assert_eq!(ids, vec!["kimi".to_string(), "octos".to_string()]);
            assert!(discover_profiles(&base.join("nonexistent")).is_empty());
            let _ = std::fs::remove_dir_all(&base);
        });
    }
}
