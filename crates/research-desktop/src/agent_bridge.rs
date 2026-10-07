//! Agent 桥接层：将 GUI 意图（ai::Intent）映射为 OUP 命令，管理 octos session 生命周期。
//!
//! 架构对齐 core-06 M1：单 Agent 对话通路——对话 → OUP turn/start → octos serve →
//! 结果写回 SQLite 项目状态。
//!
//! CPU 红线：OUP 客户端阻塞等待响应，但仅在用户提交对话时触发；
//! 无活跃 turn 时不占用 CPU。

use serde_json::Value;

use crate::oup::{OupClient, OupConfig, OupError};

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
    /// 返回 (turn_id, completed_result)。阻塞直到 turn/completed 或超时。
    /// CPU 红线：仅在用户主动提交时调用，不用于轮询。
    pub fn submit_and_wait(&mut self, prompt: &str, timeout_secs: u64) -> AgentResult<(String, Value)> {
        if self.state == AgentSessionState::TurnActive {
            return Err(AgentError::TurnAlreadyActive);
        }

        let client = self.client.as_mut().ok_or(AgentError::NotConnected)?;

        // 启动 turn
        let start_result = client.turn_start(&self.config.session_id, prompt)?;
        let turn_id = start_result
            .get("turn_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        self.state = AgentSessionState::TurnActive;

        // 等待完成
        let completed = client.turn_wait_completed(&self.config.session_id, &turn_id, timeout_secs)?;

        self.state = AgentSessionState::Connected;
        Ok((turn_id, completed))
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
}
