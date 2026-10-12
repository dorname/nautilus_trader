//! OUP（Octos UI Protocol）客户端：research-desktop ↔ octos serve 的 stdio JSON-RPC 通道。
//!
//! 架构对齐 core-06：GUI 不内嵌 agent，而是通过 AppUiCommand/JSON-RPC 与
//! `octos serve --stdio` 通信。本模块只负责协议编解码与进程生命周期，
//! 业务语义由上层（`agent_bridge`）承载。
//!
//! CPU 红线（对齐 pipeline.rs）：静默零帧——无活跃任务时不轮询；
//! 读线程阻塞在 stdout read，有数据才唤醒 GUI。
//!
//! stdio 能力子集（issue #20）：`octos serve --stdio` 未配置 REST handler，
//! `session/messages_page` 不可用（503）。桌面读取 turn 产物的权威路径是
//! notification（`turn/completed` / `assistant_persisted` /
//! `projection/envelope`）与 `turn/state/get`；不得把 REST 风格
//! `messages_page` 当作桌面读历史/产物的主路径。

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
    /// 工作区根目录（子进程 cwd，issue #14）。
    pub workspace_root: String,
    /// 额外注入子进程的环境变量（如 profile `config.env_vars` 的 LLM 凭据，
    /// 由 AgentBridge::connect 解析填充；禁止写仓库，issue #14）。
    pub extra_env: Vec<(String, String)>,
    /// 闭环权限档（embed-octos-runtime）：默认 false = Workspace-Write
    /// （工作区内免审批、文件系统仍沙箱）。true 追加
    /// `--solo --danger-full-access`（沙箱禁用 + 审批永不提示）——
    /// 仅允许人类在 GUI 显式二次确认后置 true，任何默认路径不得开启。
    pub danger_full_access: bool,
}

impl Default for OupConfig {
    fn default() -> Self {
        let base = std::env::temp_dir().join("research-desktop-octos");
        Self {
            octos_bin: "octos".into(),
            data_dir: base.join("data").to_string_lossy().into(),
            instance_dir: base.join("instance").to_string_lossy().into(),
            workspace_root: ".".into(),
            extra_env: Vec::new(),
            danger_full_access: false,
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
    /// 积压的工具执行事件（embed-octos-runtime；poll_turn_once 收集，
    /// take_tool_events 收割）。
    pending_tool_events: Vec<ToolEvent>,
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

/// 工具执行事件（embed-octos-runtime）：`tool/started`/`tool/completed`
/// 通知的结构化视图，供任务条呈现 agent 工具执行行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEvent {
    /// 所属会话。
    pub session_id: String,
    /// 所属 turn。
    pub turn_id: String,
    /// 工具调用 ID（started/completed 按 ID 配对；缺失为空串）。
    pub tool_call_id: String,
    /// 工具名（通知未携带时为 None，由上层与 started 事件合并）。
    pub name: Option<String>,
    /// `true` = tool/completed（执行完毕）；`false` = tool/started。
    pub completed: bool,
}

/// JSON-RPC 响应（只关心 result/error）。
#[derive(Debug, Deserialize)]
pub struct RpcResponse {
    pub result: Option<Value>,
    pub error: Option<RpcError>,
}

/// turn 终态结果：`turn/completed` 参数 + 本 turn 持久化的助手回复文本。
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    /// `turn/completed` 通知参数（含 tokens_in/out、session_result、cursor 等）。
    pub completed: Value,
    /// `assistant_persisted` 投影的回复文本（RD-018：只取持久化产物，
    /// 禁止拼接 message/delta）；模型未产出文本时为 None。
    pub assistant_text: Option<String>,
}

/// turn 终态（非阻塞轮询口径，对齐 issue #12 修复）：
/// 判定依据为 `turn/completed` / `turn/error` 事件流，禁止以 task/list 空列表冒充成功。
#[derive(Debug, Clone)]
pub enum TurnTerminal {
    /// `turn/completed`：含通知参数与持久化回复文本。
    Completed(TurnOutcome),
    /// `turn/error`：错误描述文本。
    Failed(String),
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
        // embed-octos-runtime：spawn 前置 backend_ensure——三级解析
        // （内嵌资源 → ~/.octos/bin → PATH）+ REQUIRED_OCTOS_RELEASE 版本 pin；
        // 缺失/不匹配给出中文诊断并拒绝 spawn（不静默继续）。
        let resolved = crate::backend_ensure::ensure()?;
        if resolved.bin != config.octos_bin {
            eprintln!(
                "[oup] octos 来源：{}（{}）",
                resolved.source.label(),
                resolved.bin
            );
        }
        let mut config = config.clone();
        config.octos_bin = resolved.bin;
        // 确保目录存在
        std::fs::create_dir_all(&config.data_dir)?;
        std::fs::create_dir_all(&config.instance_dir)?;

        // issue #14：stderr 落盘可排障（原为 Stdio::null 静默吞掉）
        let log_dir = std::path::Path::new(&config.data_dir).join("logs");
        std::fs::create_dir_all(&log_dir)?;
        let stderr_log = std::fs::File::create(log_dir.join("octos-stderr.log"))?;

        let mut cmd = Command::new(&config.octos_bin);
        cmd
            .args([
                "serve",
                "--stdio",
                "--data-dir",
                &config.data_dir,
                "--instance-data-dir",
                &config.instance_dir,
            ])
            // embed-octos-runtime：Full-Access 权限档（人类显式决策项，
            // 默认 Workspace-Write 免审批档不携带任何免沙箱标志）。
            .args(if config.danger_full_access {
                vec!["--solo", "--danger-full-access"]
            } else {
                Vec::<&str>::new()
            })
            .env("OCTOS_HOME", &config.data_dir)
            // issue #14：workspace_root 作为子进程 cwd（原为未使用字段）
            .current_dir(&config.workspace_root)
            .envs(config.extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_log));
        let mut child = cmd.spawn().map_err(OupError::Spawn)?;

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
            pending_tool_events: Vec::new(),
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

    /// 非阻塞泵一次本 turn 的事件流：排空两个通道的当前积压消息，
    /// 返回已到达的终态（未到达返回 `Ok(None)`）。
    ///
    /// `assistant_text` 由调用方跨调用持有：本 turn 的 `assistant_persisted`
    /// 文本可能早于终态通知到达，逐次累积进该参数，终态时随 [`TurnOutcome`] 带出。
    /// embed-octos-runtime：扫到 `tool/started`/`tool/completed` 时结构化积压，
    /// 由 [`OupClient::take_tool_events`] 收割（agent 工具执行对 GUI 可见）。
    pub fn poll_turn_once(
        &mut self,
        session_id: &str,
        turn_id: &str,
        assistant_text: &mut Option<String>,
    ) -> OupResult<Option<TurnTerminal>> {
        // 只处理当前已到达的消息，绝不等待（CPU 红线：轮询方自行控制节奏）
        let mut backlog: Vec<Value> = Vec::new();
        while let Ok(msg) = self.notification_rx.try_recv() {
            backlog.push(msg);
        }
        while let Ok(msg) = self.rx.try_recv() {
            backlog.push(msg);
        }
        // 两遍扫描（issue #16 收尾修复）：先收全本批所有 assistant_persisted，
        // 再找终态——单遍提前 return 会在「persisted 与 completed 同批但排在
        // 其后」（双通道排空顺序不保证全局时序）时丢失产物文本。
        for msg in &backlog {
            if let Some(text) = Self::match_assistant_persisted(msg, session_id, turn_id) {
                *assistant_text = Some(text);
            }
            if let Some(event) = Self::parse_tool_event(msg) {
                self.pending_tool_events.push(event);
            }
        }
        for msg in backlog {
            if let Some(outcome) = Self::match_turn_terminal(Some(msg), session_id, turn_id)? {
                return Ok(Some(match outcome {
                    Ok(completed) => TurnTerminal::Completed(TurnOutcome {
                        completed,
                        assistant_text: assistant_text.clone(),
                    }),
                    Err(e) => TurnTerminal::Failed(e.to_string()),
                }));
            }
        }
        Ok(None)
    }

    /// 收割积压的工具执行事件（embed-octos-runtime）：非阻塞，取走即清。
    pub fn take_tool_events(&mut self) -> Vec<ToolEvent> {
        std::mem::take(&mut self.pending_tool_events)
    }

    /// 解析 `tool/started` / `tool/completed` 通知为结构化事件；
    /// 其他消息返回 None（未知通知不 panic、不中断读循环）。
    fn parse_tool_event(msg: &Value) -> Option<ToolEvent> {
        let method = msg.get("method").and_then(|m| m.as_str())?;
        let completed = match method {
            "tool/started" => false,
            "tool/completed" => true,
            _ => return None,
        };
        let params = msg.get("params")?;
        Some(ToolEvent {
            session_id: params.get("session_id")?.as_str()?.into(),
            turn_id: params.get("turn_id")?.as_str()?.into(),
            tool_call_id: params
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into(),
            name: params
                .get("tool")
                .and_then(|v| v.as_str())
                .or_else(|| params.get("name").and_then(|v| v.as_str()))
                .map(ToString::to_string),
            completed,
        })
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

    /// 中断活跃 turn（OUP turn/interrupt，issue #19）。
    ///
    /// 真实探针语义：服务端返回 `{interrupted:true}`，随后该 turn 以
    /// `turn/error(code=interrupted)` 收尾；`turn/state/get` 为 `interrupted`。
    /// 与 `task/cancel` 不同，本方法对活跃 LLM turn 真实生效。
    pub fn turn_interrupt(&mut self, session_id: &str, turn_id: &str) -> OupResult<Value> {
        self.call(
            "turn/interrupt",
            serde_json::json!({
                "session_id": session_id,
                "turn_id": turn_id,
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
    /// 等待期间捕获本 turn 的 `assistant_persisted` 投影文本（RD-018：
    /// 回复只取持久化产物，不拼接 message/delta）。
    ///
    /// 超时返回 [`OupError::Timeout`]；读端断开返回 [`OupError::ProcessExited`]。
    pub fn turn_wait_completed(&mut self, session_id: &str, turn_id: &str, timeout_secs: u64) -> OupResult<TurnOutcome> {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let mut assistant_text: Option<String> = None;

        loop {
            if start.elapsed() > timeout {
                return Err(OupError::Timeout);
            }

            // 先排空已转发的通知，再试 rx（accepted 后事件留在 rx）。
            // embed-octos-runtime：同步收集 tool/started|completed（与
            // poll_turn_once 口径一致，submit_and_wait 路径的工具可见性）。
            if let Ok(msg) = self.notification_rx.try_recv() {
                if let Some(text) = Self::match_assistant_persisted(&msg, session_id, turn_id) {
                    assistant_text = Some(text);
                }
                if let Some(event) = Self::parse_tool_event(&msg) {
                    self.pending_tool_events.push(event);
                }
                if let Some(outcome) = Self::match_turn_terminal(Some(msg), session_id, turn_id)? {
                    // issue #37：completed 但文本空时进宽限窗口收割迟到的 persisted
                    let completed = outcome?;
                    if assistant_text.is_none() {
                        drain_persisted_grace(&self.rx, &self.notification_rx, &mut self.pending_tool_events, session_id, turn_id, &mut assistant_text, PERSISTED_GRACE);
                    }
                    return Ok(TurnOutcome { completed, assistant_text });
                }
            }
            if let Ok(msg) = self.rx.try_recv() {
                if let Some(text) = Self::match_assistant_persisted(&msg, session_id, turn_id) {
                    assistant_text = Some(text);
                }
                if let Some(event) = Self::parse_tool_event(&msg) {
                    self.pending_tool_events.push(event);
                }
                if let Some(outcome) = Self::match_turn_terminal(Some(msg), session_id, turn_id)? {
                    // issue #37：completed 但文本空时进宽限窗口收割迟到的 persisted
                    let completed = outcome?;
                    if assistant_text.is_none() {
                        drain_persisted_grace(&self.rx, &self.notification_rx, &mut self.pending_tool_events, session_id, turn_id, &mut assistant_text, PERSISTED_GRACE);
                    }
                    return Ok(TurnOutcome { completed, assistant_text });
                }
            }

            // 短阻塞：优先等 notification；超时后回环再泵 rx，避免空转打满 CPU
            match self.notification_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(msg) => {
                    if let Some(text) = Self::match_assistant_persisted(&msg, session_id, turn_id) {
                        assistant_text = Some(text);
                    }
                    if let Some(event) = Self::parse_tool_event(&msg) {
                        self.pending_tool_events.push(event);
                    }
                    if let Some(outcome) = Self::match_turn_terminal(Some(msg), session_id, turn_id)? {
                        // issue #37：completed 但文本空时进宽限窗口
                        let completed = outcome?;
                        if assistant_text.is_none() {
                            drain_persisted_grace(&self.rx, &self.notification_rx, &mut self.pending_tool_events, session_id, turn_id, &mut assistant_text, PERSISTED_GRACE);
                        }
                        return Ok(TurnOutcome { completed, assistant_text });
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // notification 发送端已丢弃时仍可能只剩 rx 上的终态事件
                    match self.rx.try_recv() {
                        Ok(msg) => {
                            if let Some(text) = Self::match_assistant_persisted(&msg, session_id, turn_id) {
                                assistant_text = Some(text);
                            }
                            if let Some(event) = Self::parse_tool_event(&msg) {
                                self.pending_tool_events.push(event);
                            }
                            if let Some(outcome) =
                                Self::match_turn_terminal(Some(msg), session_id, turn_id)?
                            {
                                // issue #37：completed 但文本空时进宽限窗口
                                let completed = outcome?;
                                if assistant_text.is_none() {
                                    drain_persisted_grace(&self.rx, &self.notification_rx, &mut self.pending_tool_events, session_id, turn_id, &mut assistant_text, PERSISTED_GRACE);
                                }
                                return Ok(TurnOutcome { completed, assistant_text });
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

    /// 若 `msg` 是本会话的 `assistant_persisted` 投影，返回持久化回复文本。
    ///
    /// embed-octos-runtime 修正：匹配放宽到 **session 级**——服务端对
    /// `projection/envelope` 的 `turn_id` 回显不稳定（实测同一二进制：新会话
    /// echo 客户端 UUID，负载/复用路径下改用内部 thread id，且 v1 投影干脆
    /// 不带 turn_id），按 UUID 严格匹配会静默丢产物（m3 pipeline 实例
    /// ST-S18-16：LLM 回复完整、通知已发、客户端匹配不上）。桌面与 swarm
    /// 均为单会话单活跃 turn（worker 单活跃口径、pipeline 串行），终态前的
    /// persisted 即属于当前 turn；每 turn 开始时 pending 文本重置，上一轮
    /// 迟到 persisted 不会串轮。`turn_id` 参数保留签名兼容，仅用于调试。
    fn match_assistant_persisted(msg: &Value, session_id: &str, _turn_id: &str) -> Option<String> {
        if msg.get("method").and_then(|m| m.as_str()) != Some("projection/envelope") {
            return None;
        }
        let params = msg.get("params")?;
        if params.get("session_id").and_then(|s| s.as_str()) != Some(session_id) {
            return None;
        }
        let payload = params.get("payload")?;
        if payload.get("type").and_then(|t| t.as_str()) != Some("assistant_persisted") {
            return None;
        }
        payload.get("data")?.get("text")?.as_str().map(ToString::to_string)
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

    /// 子进程 PID（测试可达：UT-S17-24 经 /proc 校验 cwd）。
    #[doc(hidden)]
    pub fn child_pid(&self) -> u32 {
        self.child.id()
    }
}

/// `turn/completed` 到达但 `assistant_text` 为空时的宽限窗口
///（fix-reallm-stability-20261012 / issue #37）：服务端可能先推
/// `turn/completed`、后落盘再推 `assistant_persisted`，立即返回会静默丢产物
///（m3 pipeline 偶发空产物的根因）。3s 覆盖本地观测到的落盘延迟，
/// 且不显著拖慢真空回复场景。
pub(crate) const PERSISTED_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// 宽限收割（issue #37）：`turn/completed` 已到但 `assistant_text` 为空时，
/// 在宽限窗口内继续泵 `rx` / `notification_rx`，收割迟到的
/// `assistant_persisted`（同时收集 tool 事件，口径与主等待循环一致）。
/// 文本到达、notification 通道断开（排空 rx 残余后）或期满即返回。
///
/// 独立为自由函数：UT 可直接注入 channel 验证，无需构造带子进程的
/// [`OupClient`]。仅访问关联函数，不触碰子进程状态。
fn drain_persisted_grace(
    rx: &mpsc::Receiver<Value>,
    notification_rx: &mpsc::Receiver<Value>,
    pending_tool_events: &mut Vec<ToolEvent>,
    session_id: &str,
    turn_id: &str,
    assistant_text: &mut Option<String>,
    grace: std::time::Duration,
) {
    let deadline = std::time::Instant::now() + grace;
    while assistant_text.is_none() {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        if remain.is_zero() {
            return;
        }
        // 分片等待（≤100ms）：期满检查及时，且给 rx 通道留泵入窗口
        let slice = remain.min(std::time::Duration::from_millis(100));
        match notification_rx.recv_timeout(slice) {
            Ok(msg) => {
                if let Some(text) = OupClient::match_assistant_persisted(&msg, session_id, turn_id) {
                    *assistant_text = Some(text);
                }
                if let Some(event) = OupClient::parse_tool_event(&msg) {
                    pending_tool_events.push(event);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // 排空 rx 上积压（accepted 后到达的事件留在 rx，与主循环口径一致）
        while let Ok(msg) = rx.try_recv() {
            if let Some(text) = OupClient::match_assistant_persisted(&msg, session_id, turn_id) {
                *assistant_text = Some(text);
            }
            if let Some(event) = OupClient::parse_tool_event(&msg) {
                pending_tool_events.push(event);
            }
        }
    }
    // notification 断开或文本到达后，最后排空一次 rx 残余
    while let Ok(msg) = rx.try_recv() {
        if let Some(text) = OupClient::match_assistant_persisted(&msg, session_id, turn_id) {
            *assistant_text = Some(text);
        }
        if let Some(event) = OupClient::parse_tool_event(&msg) {
            pending_tool_events.push(event);
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

    /// UT-oup-04：assistant_persisted 投影提取回复文本（RD-018 口径）。
    #[test]
    fn assistant_persisted_extracts_text() {
        let msg = serde_json::json!({
            "method": "projection/envelope",
            "params": {
                "session_id": "s1",
                "turn_id": "t1",
                "payload": {
                    "type": "assistant_persisted",
                    "data": { "text": "ok", "assistant_segment_id": "t1:assistant:1" }
                }
            }
        });
        assert_eq!(
            OupClient::match_assistant_persisted(&msg, "s1", "t1"),
            Some("ok".to_string())
        );
    }

    /// UT-oup-05：user_message 投影 / session 不匹配 / 非 envelope 均不提取；
    /// embed-octos-runtime 语义修订：turn_id 与请求不一致（服务端 echo 不稳定，
    /// 可能改用内部 thread id 或缺失）但 session 匹配的 persisted **仍提取**
    /// （单会话单活跃 turn 口径，详见 `match_assistant_persisted` 文档）。
    #[test]
    fn assistant_persisted_rejects_non_matching() {
        let user_msg = serde_json::json!({
            "method": "projection/envelope",
            "params": {
                "session_id": "s1",
                "turn_id": "t1",
                "payload": { "type": "user_message", "data": { "text": "hi" } }
            }
        });
        assert_eq!(OupClient::match_assistant_persisted(&user_msg, "s1", "t1"), None);

        let msg = serde_json::json!({
            "method": "projection/envelope",
            "params": {
                "session_id": "s1",
                "turn_id": "server-thread-id",
                "payload": { "type": "assistant_persisted", "data": { "text": "ok" } }
            }
        });
        // turn_id 为服务端内部 id（echo 失稳）：session 匹配即提取。
        assert_eq!(
            OupClient::match_assistant_persisted(&msg, "s1", "client-uuid"),
            Some("ok".to_string())
        );
        // v1 投影无 turn_id 字段：session 匹配即提取。
        let v1 = serde_json::json!({
            "method": "projection/envelope",
            "params": {
                "session_id": "s1",
                "payload": { "type": "assistant_persisted", "data": { "text": "ok" } }
            }
        });
        assert_eq!(
            OupClient::match_assistant_persisted(&v1, "s1", "client-uuid"),
            Some("ok".to_string())
        );
        // session 不匹配：拒绝（跨会话隔离仍然严格）。
        assert_eq!(OupClient::match_assistant_persisted(&msg, "s2", "server-thread-id"), None);

        let delta = serde_json::json!({
            "method": "message/delta",
            "params": { "session_id": "s1", "turn_id": "t1", "text": "o" }
        });
        assert_eq!(OupClient::match_assistant_persisted(&delta, "s1", "t1"), None);
    }
    /// UT-S18-17（embed-octos-runtime）：工具通知解析——`tool/started`/
    /// `tool/completed` 结构化（tool_call_id、名称、终态）；未知通知不 panic。
    #[test]
    fn tool_events_parse_structured() {
        nautilus_research_testkit::case("UT-S18-17", || {
            let started = serde_json::json!({
                "method": "tool/started",
                "params": { "session_id": "s1", "turn_id": "t1",
                            "tool_call_id": "call_1", "tool": "write_file" }
            });
            let ev = OupClient::parse_tool_event(&started).expect("started 应解析");
            assert_eq!(ev.session_id, "s1");
            assert_eq!(ev.turn_id, "t1");
            assert_eq!(ev.tool_call_id, "call_1");
            assert_eq!(ev.name.as_deref(), Some("write_file"));
            assert!(!ev.completed, "tool/started 的 completed=false");

            let done = serde_json::json!({
                "method": "tool/completed",
                "params": { "session_id": "s1", "turn_id": "t1",
                            "tool_call_id": "call_1", "name": "write_file" }
            });
            let ev2 = OupClient::parse_tool_event(&done).expect("completed 应解析");
            assert!(ev2.completed, "tool/completed 的 completed=true");
            // name 字段兼容（tool 或 name 键）
            assert_eq!(ev2.name.as_deref(), Some("write_file"));

            // 未知通知/缺失字段：None（不 panic、不中断读循环）
            let other = serde_json::json!({
                "method": "message/delta",
                "params": { "session_id": "s1", "text": "x" }
            });
            assert_eq!(OupClient::parse_tool_event(&other), None);
            let bad = serde_json::json!({ "method": "tool/started", "params": {} });
            assert_eq!(OupClient::parse_tool_event(&bad), None);
        });
    }

    /// UT-S18-18（embed-octos-runtime）：闭环权限默认档——OupConfig 默认
    /// 携带 Workspace-Write（danger_full_access=false）；Full-Access 只能
    /// 由调用方显式置位（GUI 两段式确认）。
    #[test]
    fn default_config_is_workspace_write() {
        nautilus_research_testkit::case("UT-S18-18", || {
            let c = OupConfig::default();
            assert!(!c.danger_full_access, "默认必须为 Workspace-Write 免审批档");
        });
    }

    /// UT-S18-19（fix-reallm-stability-20261012 / issue #37）：completed 先到、
    /// assistant_text 为空时，宽限窗口内收割迟到的 assistant_persisted；
    /// 窗口内 tool 事件不丢失。
    #[test]
    fn ut_s18_19_grace_collects_late_persisted() {
        nautilus_research_testkit::case("UT-S18-19", || {
            let (_tx, rx) = mpsc::channel::<Value>();
            let (ntx, nrx) = mpsc::channel::<Value>();
            let mut tools = Vec::new();
            let mut text: Option<String> = None;
            // 模拟服务端时序：150ms 后迟到的 tool/completed + assistant_persisted
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                let _ = ntx.send(serde_json::json!({
                    "method": "tool/completed",
                    "params": { "session_id": "s1", "turn_id": "t1",
                                "tool_call_id": "call_9", "name": "write_file" }
                }));
                let _ = ntx.send(serde_json::json!({
                    "method": "projection/envelope",
                    "params": {
                        "session_id": "s1",
                        "payload": { "type": "assistant_persisted",
                                     "data": { "text": "迟到回复" } }
                    }
                }));
            });
            drain_persisted_grace(
                &rx, &nrx, &mut tools, "s1", "t1", &mut text,
                std::time::Duration::from_secs(3),
            );
            assert_eq!(text.as_deref(), Some("迟到回复"), "宽限窗口应收割迟到的 persisted");
            assert!(
                tools.iter().any(|e| e.completed && e.tool_call_id == "call_9"),
                "宽限窗口内 tool 事件不应丢失"
            );
        });
    }

    /// UT-S18-20（fix-reallm-stability-20261012 / issue #37）：宽限期满仍无
    /// persisted → 及时返回空文本；notification 通道断开 → 排空 rx 后提前
    /// 结束（不等满整个宽限窗口）。
    #[test]
    fn ut_s18_20_grace_expires_empty() {
        nautilus_research_testkit::case("UT-S18-20", || {
            // 期满语义：无消息，grace=200ms 应及时返回且文本仍空
            let (_tx, rx) = mpsc::channel::<Value>();
            let (_ntx, nrx) = mpsc::channel::<Value>();
            let mut tools = Vec::new();
            let mut text: Option<String> = None;
            let t0 = std::time::Instant::now();
            drain_persisted_grace(
                &rx, &nrx, &mut tools, "s1", "t1", &mut text,
                std::time::Duration::from_millis(200),
            );
            let elapsed = t0.elapsed();
            assert!(text.is_none(), "期满无 persisted 时文本仍为空");
            assert!(
                elapsed < std::time::Duration::from_secs(2),
                "期满应及时返回（不无限等待），实测 {elapsed:?}"
            );

            // 断开语义：notification 发送端断开 → 排空 rx 后立即结束
            let (_tx2, rx2) = mpsc::channel::<Value>();
            let (ntx2, nrx2) = mpsc::channel::<Value>();
            drop(ntx2);
            let t1 = std::time::Instant::now();
            drain_persisted_grace(
                &rx2, &nrx2, &mut tools, "s1", "t1", &mut text,
                std::time::Duration::from_secs(3),
            );
            let d = t1.elapsed();
            assert!(d < std::time::Duration::from_secs(1), "通道断开应提前结束，实测 {d:?}");
            assert!(text.is_none());
        });
    }
}
