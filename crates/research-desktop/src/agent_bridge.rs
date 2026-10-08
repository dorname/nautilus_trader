//! Agent 桥接层：将 GUI 意图（ai::Intent）映射为 OUP 命令，管理 octos session 生命周期。
//!
//! 架构对齐 core-06 M1：单 Agent 对话通路——对话 → OUP turn/start → octos serve →
//! 结果写回 SQLite 项目状态。
//!
//! CPU 红线：OUP 客户端阻塞等待响应，但仅在用户提交对话时触发；
//! 无活跃 turn 时不占用 CPU。

use serde_json::Value;

use crate::oup::{OupClient, OupConfig, OupError, TurnOutcome};

/// 真实 LLM 调用默认超时（秒），对齐 wire-real-llm-oup 提案。
pub const DEFAULT_TURN_TIMEOUT_SECS: u64 = 120;

/// Agent 会话配置。
#[derive(Debug, Clone)]
pub struct AgentSessionConfig {
    /// 会话 ID（与 research-desktop 项目 ID 绑定）。
    pub session_id: String,
    /// Profile ID（octos 运行时配置）。
    pub profile_id: String,
    /// OUP 客户端配置。
    pub oup: OupConfig,
}

impl Default for AgentSessionConfig {
    fn default() -> Self {
        Self {
            session_id: "research-default".into(),
            profile_id: "octos".into(),
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
    pub fn desktop_with_profile(
        base_dir: &std::path::Path,
        project_id: &str,
        profile_src: Option<&std::path::Path>,
    ) -> Self {
        let data_dir = base_dir.join("data");
        let instance_dir = base_dir.join("instance");
        let profile_dst_dir = data_dir.join("profiles");
        let _ = std::fs::create_dir_all(&profile_dst_dir);
        let _ = std::fs::create_dir_all(&instance_dir);
        let profile_dst = profile_dst_dir.join("octos.json");
        if !profile_dst.exists() {
            if let Some(src) = profile_src {
                let _ = std::fs::copy(src, &profile_dst);
            }
        }
        Self {
            session_id: format!("research-{project_id}"),
            profile_id: "octos".into(),
            oup: OupConfig {
                octos_bin: "octos".into(),
                data_dir: data_dir.to_string_lossy().into(),
                instance_dir: instance_dir.to_string_lossy().into(),
                workspace_root: ".".into(),
            },
        }
    }
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
}

/// Agent 操作结果。
pub type AgentResult<T> = Result<T, AgentError>;

/// Agent 错误。
#[derive(Debug)]
pub enum AgentError {
    /// OUP 通信错误。
    Oup(OupError),
    /// 会话未连接。
    NotConnected,
    /// 会话已存在活跃 turn。
    TurnAlreadyActive,
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Oup(e) => write!(f, "OUP 错误: {e}"),
            AgentError::NotConnected => write!(f, "Agent 会话未连接"),
            AgentError::TurnAlreadyActive => write!(f, "已有活跃对话进行中"),
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
        }
    }

    /// 连接 octos serve（启动子进程并打开 session）。
    pub fn connect(&mut self) -> AgentResult<()> {
        if self.state != AgentSessionState::Disconnected {
            return Ok(()); // 幂等
        }

        let mut client = OupClient::spawn(&self.config.oup)?;
        client.session_open_with_profile(&self.config.session_id, &self.config.profile_id)?;

        self.client = Some(client);
        self.state = AgentSessionState::Connected;
        self.last_error = None;
        Ok(())
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

        self.state = AgentSessionState::TurnActive;
        Ok(turn_id)
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

        // 等待完成（同时泵 rx + notification_rx，避免 turn/completed 饿死）
        let outcome = match client.turn_wait_completed(&self.config.session_id, &turn_id, timeout_secs)
        {
            Ok(v) => v,
            Err(e) => {
                self.state = AgentSessionState::Connected;
                return Err(AgentError::Oup(e));
            }
        };

        self.state = AgentSessionState::Connected;
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

    /// 列出 agents（agent/list）。
    pub fn list_agents(&mut self) -> AgentResult<Value> {
        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;
        Ok(client.agent_list(&self.config.session_id)?)
    }

    /// 标记 turn 完成（由事件循环或超时调用）。
    pub fn complete_turn(&mut self) {
        if self.state == AgentSessionState::TurnActive {
            self.state = AgentSessionState::Connected;
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

/// Worker 单 turn 结果（GUI 展示口径）。
#[derive(Debug, Clone)]
pub struct WorkerTurn {
    /// 合法 UUID turn_id。
    pub turn_id: String,
    /// assistant_persisted 回复文本（模型未产出文本时为 None）。
    pub reply: Option<String>,
    /// 输入 token 数（turn/completed 口径）。
    pub tokens_in: Option<u64>,
    /// 输出 token 数。
    pub tokens_out: Option<u64>,
}

/// Worker 响应：任务 ID + 结果（Err 为可展示错误文本）。
#[derive(Debug)]
pub struct WorkerResponse {
    /// 提交时分配的任务序号（过期结果由调用方比对丢弃）。
    pub id: u64,
    /// turn 结果或错误文本。
    pub result: Result<WorkerTurn, String>,
}

/// Worker 请求：任务 ID + prompt。
#[derive(Debug)]
struct WorkerRequest {
    id: u64,
    prompt: String,
}

/// 后台 Agent worker：独立线程持有 AgentBridge（含 octos serve 子进程），
/// GUI 帧循环经 channel 收发，不阻塞渲染。octos 惰性连接：首个任务到来才 spawn。
pub struct AgentWorker {
    tx: Option<std::sync::mpsc::Sender<WorkerRequest>>,
    rx: std::sync::mpsc::Receiver<WorkerResponse>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl AgentWorker {
    /// 启动 worker 线程（此时不连接 octos；首个 submit 才建立会话）。
    pub fn spawn(config: AgentSessionConfig) -> Self {
        let (req_tx, req_rx) = std::sync::mpsc::channel::<WorkerRequest>();
        let (resp_tx, resp_rx) = std::sync::mpsc::channel::<WorkerResponse>();
        let handle = std::thread::spawn(move || Self::run(config, req_rx, resp_tx));
        Self {
            tx: Some(req_tx),
            rx: resp_rx,
            handle: Some(handle),
        }
    }

    fn run(
        config: AgentSessionConfig,
        req_rx: std::sync::mpsc::Receiver<WorkerRequest>,
        resp_tx: std::sync::mpsc::Sender<WorkerResponse>,
    ) {
        let mut bridge: Option<AgentBridge> = None;
        while let Ok(req) = req_rx.recv() {
            if bridge.is_none() {
                let mut b = AgentBridge::new(config.clone());
                match b.connect() {
                    Ok(()) => bridge = Some(b),
                    Err(e) => {
                        let _ = resp_tx.send(WorkerResponse {
                            id: req.id,
                            result: Err(format!("连接 octos 失败：{e}")),
                        });
                        continue;
                    }
                }
            }
            let b = bridge.as_mut().expect("bridge 已在上方建立");
            let result = b
                .submit_and_wait(&req.prompt, DEFAULT_TURN_TIMEOUT_SECS)
                .map(|(turn_id, out)| WorkerTurn {
                    turn_id,
                    reply: out.assistant_text,
                    tokens_in: out.completed.get("tokens_in").and_then(|v| v.as_u64()),
                    tokens_out: out.completed.get("tokens_out").and_then(|v| v.as_u64()),
                })
                .map_err(|e| e.to_string());
            if resp_tx.send(WorkerResponse { id: req.id, result }).is_err() {
                return;
            }
        }
    }

    /// 提交任务（非阻塞；worker 串行执行，真实 LLM 调用带 120s 超时）。
    pub fn submit(&self, id: u64, prompt: String) -> Result<(), String> {
        let tx = self.tx.as_ref().ok_or("agent worker 已关闭")?;
        tx.send(WorkerRequest { id, prompt })
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
}
