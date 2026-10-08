//! OUP（Octos UI Protocol）客户端：research-desktop ↔ octos serve 的 stdio JSON-RPC 通道。
//!
//! 架构对齐 core-06：GUI 不内嵌 agent，而是通过 AppUiCommand/JSON-RPC 与
//! `octos serve --stdio` 通信。本模块只负责协议编解码与进程生命周期，
//! 业务语义由上层（`agent_bridge`）承载。
//!
//! CPU 红线（对齐 pipeline.rs）：静默零帧——无活跃任务时不轮询；
//! 读线程阻塞在 stdout read，有数据才唤醒 GUI。

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 请求 ID 前缀（避免与其他客户端冲突）。
const ID_PREFIX: &str = "rd";

/// OUP 协议版本标识。
pub const OUP_PROTOCOL: &str = "octos-ui/v1alpha1";

/// OUP 客户端配置。
#[derive(Debug, Clone)]
pub struct OupConfig {
    /// octos 二进制路径。
    pub octos_bin: String,
    /// 独立数据目录（避免与主 octos 实例锁冲突）。
    pub data_dir: String,
    /// 独立实例目录。
    pub instance_dir: String,
    /// 工作区根目录。
    pub workspace_root: String,
}

impl Default for OupConfig {
    fn default() -> Self {
        let base = std::env::temp_dir().join("research-desktop-octos");
        Self {
            octos_bin: "octos".into(),
            data_dir: base.join("data").to_string_lossy().into(),
            instance_dir: base.join("instance").to_string_lossy().into(),
            workspace_root: ".".into(),
        }
    }
}

/// OUP 客户端：持有 octos serve 子进程，提供 JSON-RPC 请求/响应通道。
pub struct OupClient {
    child: Child,
    stdin: ChildStdin,
    /// 响应接收端（读线程 → GUI 主线程）。
    rx: mpsc::Receiver<Value>,
    /// 通知接收端（读线程 → 事件循环）。
    notification_rx: mpsc::Receiver<Value>,
    /// 通知发送端（call 方法转发通知 → 事件循环）。
    notification_tx: mpsc::Sender<Value>,
    /// 请求计数器（生成唯一 ID）。
    counter: u64,
    /// 读线程句柄（drop 时等待退出）。
    reader_handle: Option<std::thread::JoinHandle<()>>,
}

/// JSON-RPC 请求。
#[derive(Debug, Serialize)]
struct RpcRequest<'a> {
    jsonrpc: &'a str,
    id: String,
    method: &'a str,
    params: Value,
}

/// JSON-RPC 响应（只关心 result/error）。
#[derive(Debug, Deserialize)]
pub struct RpcResponse {
    pub result: Option<Value>,
    pub error: Option<RpcError>,
}

/// JSON-RPC 错误。
#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

/// OUP 操作结果。
pub type OupResult<T> = Result<T, OupError>;

/// OUP 错误。
#[derive(Debug)]
pub enum OupError {
    /// 进程启动失败。
    Spawn(std::io::Error),
    /// IO 错误。
    Io(std::io::Error),
    /// JSON-RPC 错误。
    Rpc(i64, String),
    /// 序列化/反序列化错误。
    Serde(serde_json::Error),
    /// 进程已退出。
    ProcessExited,
    /// 等待 turn 完成超时。
    Timeout,
}

impl std::fmt::Display for OupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OupError::Spawn(e) => write!(f, "octos 启动失败: {e}"),
            OupError::Io(e) => write!(f, "IO 错误: {e}"),
            OupError::Rpc(code, msg) => write!(f, "RPC 错误 {code}: {msg}"),
            OupError::Serde(e) => write!(f, "序列化错误: {e}"),
            OupError::ProcessExited => write!(f, "octos 进程已退出"),
            OupError::Timeout => write!(f, "等待 turn 完成超时"),
        }
    }
}

impl std::error::Error for OupError {}

impl From<std::io::Error> for OupError {
    fn from(e: std::io::Error) -> Self {
        OupError::Io(e)
    }
}

impl From<serde_json::Error> for OupError {
    fn from(e: serde_json::Error) -> Self {
        OupError::Serde(e)
    }
}

impl OupClient {
    /// 启动 octos serve 子进程并建立 stdio 通道。
    pub fn spawn(config: &OupConfig) -> OupResult<Self> {
        // 确保目录存在
        std::fs::create_dir_all(&config.data_dir)?;
        std::fs::create_dir_all(&config.instance_dir)?;

        let mut child = Command::new(&config.octos_bin)
            .args([
                "serve",
                "--stdio",
                "--data-dir",
                &config.data_dir,
                "--instance-data-dir",
                &config.instance_dir,
            ])
            .env("OCTOS_HOME", &config.data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(OupError::Spawn)?;

        let stdin = child.stdin.take().ok_or(OupError::ProcessExited)?;
        let stdout = child.stdout.take().ok_or(OupError::ProcessExited)?;

        let (tx, rx) = mpsc::channel();
        let (notification_tx, notification_rx) = mpsc::channel();

        // 读线程：阻塞读 stdout，解析 JSON-RPC 响应，发送到 channel
        let reader_handle = std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            // 所有消息都发到 rx（call 方法会区分响应和通知）
                            if tx.send(value).is_err() {
                                break; // GUI 已退出
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            rx,
            notification_rx,
            notification_tx,
            counter: 0,
            reader_handle: Some(reader_handle),
        })
    }

    /// 发送 JSON-RPC 请求，返回响应（阻塞等待，调用方需自行控制超时）。
    pub fn call(&mut self, method: &str, params: Value) -> OupResult<Value> {
        self.counter += 1;
        let id = format!("{ID_PREFIX}-{}", self.counter);

        let request = RpcRequest {
            jsonrpc: "2.0",
            id: id.clone(),
            method,
            params,
        };

        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;

        // 阻塞等待匹配 ID 的响应（notification 转发到 notification channel）
        loop {
            let msg = self.rx.recv().map_err(|_| OupError::ProcessExited)?;
            // 只处理带 id 的响应（notification 转发到 notification channel）
            if msg.get("id").and_then(|v| v.as_str()) == Some(id.as_str()) {
                let response: RpcResponse = serde_json::from_value(msg)?;
                if let Some(err) = response.error {
                    return Err(OupError::Rpc(err.code, err.message));
                }
                return Ok(response.result.unwrap_or(Value::Null));
            } else if msg.get("method").is_some() {
                // notification 转发到 notification channel（不丢弃）
                let _ = self.notification_tx.send(msg);
            }
        }
    }

    /// 非阻塞尝试接收一条消息（响应或 notification）。
    pub fn try_recv(&self) -> Option<Value> {
        self.rx.try_recv().ok()
    }

    /// 非阻塞尝试接收一条通知。
    pub fn try_recv_notification(&self) -> Option<Value> {
        self.notification_rx.try_recv().ok()
    }

    /// 打开 session（OUP session/open）。
    pub fn session_open(&mut self, session_id: &str) -> OupResult<Value> {
        self.session_open_with_profile(session_id, "octos")
    }

    /// 打开 session 并指定 profile。
    pub fn session_open_with_profile(&mut self, session_id: &str, profile_id: &str) -> OupResult<Value> {
        self.call(
            "session/open",
            serde_json::json!({
                "session_id": session_id,
                "profile_id": profile_id,
            }),
        )
    }

    /// 列出 tasks（OUP task/list）。
    pub fn task_list(&mut self, session_id: &str) -> OupResult<Value> {
        self.call(
            "task/list",
            serde_json::json!({
                "session_id": session_id,
            }),
        )
    }

    /// 取消 task（OUP task/cancel）。
    pub fn task_cancel(&mut self, session_id: &str, task_id: &str) -> OupResult<Value> {
        self.call(
            "task/cancel",
            serde_json::json!({
                "session_id": session_id,
                "task_id": task_id,
            }),
        )
    }

    /// 启动 turn（OUP turn/start）。
    ///
    /// 返回值始终含客户端生成的 `turn_id`（UUID）；服务端 `accepted` 响应里未必带回该字段。
    pub fn turn_start(&mut self, session_id: &str, prompt: &str) -> OupResult<Value> {
        let turn_id = uuid::Uuid::new_v4().to_string();
        self.turn_start_with_id(session_id, &turn_id, prompt)?;
        Ok(serde_json::json!({ "turn_id": turn_id, "accepted": true }))
    }

    /// 启动 turn 并指定 turn_id（发送请求并等待 accepted；turn/completed 由 turn_wait_completed 处理）。
    pub fn turn_start_with_id(&mut self, session_id: &str, turn_id: &str, prompt: &str) -> OupResult<Value> {
        self.counter += 1;
        let id = format!("{ID_PREFIX}-{}", self.counter);

        let request = RpcRequest {
            jsonrpc: "2.0",
            id: id.clone(),
            method: "turn/start",
            params: serde_json::json!({
                "session_id": session_id,
                "kind": "user",
                "turn_id": turn_id,
                "input": [{"kind": "text", "text": prompt}],
            }),
        };

        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;

        // 等待 turn/start 的响应（确认 turn 已启动），notification 转发到 notification channel
        loop {
            let msg = self.rx.recv().map_err(|_| OupError::ProcessExited)?;
            if msg.get("id").and_then(|v| v.as_str()) == Some(id.as_str()) {
                let response: RpcResponse = serde_json::from_value(msg)?;
                if let Some(err) = response.error {
                    return Err(OupError::Rpc(err.code, err.message));
                }
                return Ok(response.result.unwrap_or(Value::Null));
            } else if msg.get("method").is_some() {
                // notification 转发到 notification channel
                let _ = self.notification_tx.send(msg);
            }
        }
    }

    /// 列出 agents（OUP agent/list）。
    pub fn agent_list(&mut self, session_id: &str) -> OupResult<Value> {
        self.call(
            "agent/list",
            serde_json::json!({
                "session_id": session_id,
            }),
        )
    }

    /// 等待 turn 完成（阻塞直到收到 turn/completed 或 turn/error）。
    ///
    /// 同时泵 `rx` 与 `notification_rx`：读线程只写入 `rx`；`call`/`turn_start_with_id`
    /// 期间转发的通知进 `notification_rx`，但 turn/start accepted 之后到达的
    /// `turn/completed` 仍停在 `rx` 上——只读 notification 通道会饿死并误报退出。
    ///
    /// 超时返回 [`OupError::Timeout`]；读端断开返回 [`OupError::ProcessExited`]。
    pub fn turn_wait_completed(&mut self, session_id: &str, turn_id: &str, timeout_secs: u64) -> OupResult<Value> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout {
                return Err(OupError::Timeout);
            }

            // 先排空已转发的通知，再试 rx（accepted 后事件留在 rx）
            if let Some(outcome) = Self::match_turn_terminal(
                self.notification_rx.try_recv().ok(),
                session_id,
                turn_id,
            )? {
                return outcome;
            }
            if let Some(outcome) = Self::match_turn_terminal(self.rx.try_recv().ok(), session_id, turn_id)? {
                return outcome;
            }

            // 短阻塞：优先等 notification；超时后回环再泵 rx，避免空转打满 CPU
            match self.notification_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(msg) => {
                    if let Some(outcome) = Self::match_turn_terminal(Some(msg), session_id, turn_id)? {
                        return outcome;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // notification 发送端已丢弃时仍可能只剩 rx 上的终态事件
                    match self.rx.try_recv() {
                        Ok(msg) => {
                            if let Some(outcome) =
                                Self::match_turn_terminal(Some(msg), session_id, turn_id)?
                            {
                                return outcome;
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                        Err(mpsc::TryRecvError::Disconnected) => {
                            return Err(OupError::ProcessExited);
                        }
                    }
                }
            }
        }
    }

    /// 若 `msg` 是匹配的 turn 终态通知，返回 `Some(Ok/Err)`；无关消息返回 `Ok(None)`。
    fn match_turn_terminal(
        msg: Option<Value>,
        session_id: &str,
        turn_id: &str,
    ) -> OupResult<Option<OupResult<Value>>> {
        let Some(msg) = msg else {
            return Ok(None);
        };
        let Some(method) = msg.get("method").and_then(|m| m.as_str()) else {
            return Ok(None);
        };
        if method != "turn/completed" && method != "turn/error" {
            return Ok(None);
        }
        let Some(params) = msg.get("params") else {
            return Ok(None);
        };
        if params.get("turn_id").and_then(|t| t.as_str()) != Some(turn_id) {
            return Ok(None);
        }
        if params.get("session_id").and_then(|s| s.as_str()) != Some(session_id) {
            return Ok(None);
        }
        if method == "turn/error" {
            let error_msg = params
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            return Ok(Some(Err(OupError::Rpc(-32000, error_msg.to_string()))));
        }
        Ok(Some(Ok(params.clone())))
    }

    /// 关闭子进程。
    pub fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for OupClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-oup-01：OupConfig 默认值包含必要字段。
    #[test]
    fn config_default_has_fields() {
        let cfg = OupConfig::default();
        assert!(!cfg.octos_bin.is_empty());
        assert!(!cfg.data_dir.is_empty());
        assert!(!cfg.instance_dir.is_empty());
    }

    /// UT-oup-02：JSON-RPC 请求序列化格式正确。
    #[test]
    fn rpc_request_serializes() {
        let req = RpcRequest {
            jsonrpc: "2.0",
            id: "rd-1".into(),
            method: "session/list",
            params: serde_json::json!({}),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":\"rd-1\""));
        assert!(json.contains("\"method\":\"session/list\""));
    }

    /// UT-oup-03：OupError Display 实现。
    #[test]
    fn error_display() {
        let e = OupError::Rpc(-32600, "invalid request".into());
        assert!(format!("{e}").contains("-32600"));
    }
}
