//! M1 验收测试：OUP 客户端与真实 octos serve 的端到端通信。
//!
//! CTU 红线：每个测试用独立的临时数据目录，避免并发锁冲突；
//! 测试完成后清理子进程，不留孤儿进程。

use nautilus_research_desktop::oup::{OupClient, OupConfig};

fn temp_oup_config(tag: &str) -> OupConfig {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "rd-m1-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let data_dir = base.join("data");
    let instance_dir = base.join("instance");

    // 确保目录存在
    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&instance_dir).expect("create instance dir");

    // 复制 profile 配置
    let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if profile_src.exists() {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    OupConfig {
        octos_bin: "octos".into(),
        data_dir: data_dir.to_string_lossy().into(),
        instance_dir: instance_dir.to_string_lossy().into(),
        workspace_root: "/home/kyle/nautilus_trader".into(),
    }
}

/// ST-M1-01：OUP 客户端 spawn + session/open + session/list 端到端。
#[test]
fn st_m1_01_oup_spawn_session_open() {
    let config = temp_oup_config("spawn");
    let mut client = OupClient::spawn(&config).expect("spawn octos serve");

    // session/open
    let result = client
        .session_open("m1-test-session")
        .expect("session/open");
    assert!(
        result.get("opened").is_some(),
        "session/open should return opened"
    );

    // session/list（验证连接存活）
    let sessions = client
        .call("session/list", serde_json::json!({}))
        .expect("session/list");
    assert!(
        sessions.get("sessions").is_some(),
        "session/list should return sessions array"
    );

    client.shutdown();
}

/// ST-M1-02：OUP task/list 端到端。
#[test]
fn st_m1_02_oup_task_list() {
    let config = temp_oup_config("task-list");
    let mut client = OupClient::spawn(&config).expect("spawn octos serve");

    client.session_open("m1-task-session").expect("session/open");

    let tasks = client.task_list("m1-task-session").expect("task/list");
    // 空 session 应返回空任务列表或包含 tasks 字段
    assert!(
        tasks.get("tasks").is_some() || tasks.is_null(),
        "task/list should return tasks"
    );

    client.shutdown();
}

/// ST-M1-03：OUP agent/list 端到端。
#[test]
fn st_m1_03_oup_agent_list() {
    let config = temp_oup_config("agent-list");
    let mut client = OupClient::spawn(&config).expect("spawn octos serve");

    client
        .session_open("m1-agent-session")
        .expect("session/open");

    let agents = client.agent_list("m1-agent-session").expect("agent/list");
    // 应返回 agents 数组（可能为空）
    assert!(
        agents.get("agents").is_some() || agents.is_null(),
        "agent/list should return agents"
    );

    client.shutdown();
}

/// ST-M1-04：OUP 错误处理（非法方法）。
#[test]
fn st_m1_04_oup_invalid_method() {
    let config = temp_oup_config("invalid");
    let mut client = OupClient::spawn(&config).expect("spawn octos serve");

    let result = client.call("nonexistent/method", serde_json::json!({}));
    assert!(result.is_err(), "invalid method should return error");

    client.shutdown();
}

/// ST-M1-05：OUP 客户端幂等 spawn（多个客户端独立数据目录）。
#[test]
fn st_m1_05_oup_concurrent_spawn() {
    let config1 = temp_oup_config("concurrent-1");
    let config2 = temp_oup_config("concurrent-2");

    let mut client1 = OupClient::spawn(&config1).expect("spawn client 1");
    let mut client2 = OupClient::spawn(&config2).expect("spawn client 2");

    client1.session_open("session-1").expect("session/open 1");
    client2.session_open("session-2").expect("session/open 2");

    // 两个客户端互不干扰
    let r1 = client1.call("session/list", serde_json::json!({}));
    let r2 = client2.call("session/list", serde_json::json!({}));
    assert!(r1.is_ok());
    assert!(r2.is_ok());

    client1.shutdown();
    client2.shutdown();
}
