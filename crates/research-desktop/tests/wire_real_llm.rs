//! 真实 LLM 接线验收测试：turn/completed + 合法 turn_id + 禁止 demo_task。
//!
//! 验收标准（wire-real-llm-oup 提案）：
//! 1. 必须收到 turn/completed 事件（非 turn/error）
//! 2. 必须返回合法 turn_id（UUID 格式）
//! 3. 禁止 demo_task（代码中无 DemoTask/DemoAction 路径）
//!
//! 注意：ST 用例调用真实 LLM（moonshot-coding），会产生 API 调用。
//! 用例 ID 对齐 logos/resources/test/core-S17-test-cases.md（wire-real-llm-oup delta）。

use nautilus_research_desktop::agent_bridge::{AgentBridge, AgentSessionConfig};
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_testkit::case;

fn temp_agent_config(tag: &str) -> AgentSessionConfig {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = std::env::temp_dir().join(format!(
        "rd-wire-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let data_dir = base.join("data");
    let instance_dir = base.join("instance");

    std::fs::create_dir_all(&data_dir).expect("create data dir");
    std::fs::create_dir_all(&instance_dir).expect("create instance dir");

    // 复制真实 LLM profile（moonshot-coding）
    let profile_src = std::path::Path::new("/home/kyle/.octos/profiles/octos.json");
    let profile_dst_dir = data_dir.join("profiles");
    std::fs::create_dir_all(&profile_dst_dir).expect("create profiles dir");
    if profile_src.exists() {
        std::fs::copy(profile_src, profile_dst_dir.join("octos.json")).expect("copy profile");
    }

    AgentSessionConfig {
        session_id: format!("wire-{tag}"),
        profile_id: "octos".into(),
        oup: OupConfig {
            octos_bin: "octos".into(),
            data_dir: data_dir.to_string_lossy().into(),
            instance_dir: instance_dir.to_string_lossy().into(),
            workspace_root: "/home/kyle/nautilus_trader".into(),
        },
    }
}

/// 验证 UUID 格式是否合法。
fn is_valid_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

/// ST-S17-17：真实 LLM turn 全生命周期——turn/completed + 合法 turn_id + 非空回复。
#[test]
fn st_s17_17_real_llm_turn_completed() {
    case("ST-S17-17", || {
        let config = temp_agent_config("turn-completed");
        let mut bridge = AgentBridge::new(config);

        bridge.connect().expect("connect to octos serve");

        // 提交真实 LLM 调用（简短 prompt 控制成本，超时对齐提案 120s）
        let (turn_id, outcome) = bridge
            .submit_and_wait("回复 ok", 120)
            .expect("submit_and_wait should succeed");

        // 验证 turn_id 是合法 UUID
        assert!(
            is_valid_uuid(&turn_id),
            "turn_id should be valid UUID, got: {turn_id}"
        );

        // 验证结果是 turn/completed（包含 session_result 或 tokens）
        assert!(
            outcome.completed.get("session_result").is_some()
                || outcome.completed.get("tokens_in").is_some(),
            "result should be turn/completed with session_result or tokens, got: {:?}",
            outcome.completed
        );

        // 验证 assistant_persisted 提取到非空回复文本
        let reply = outcome.assistant_text.unwrap_or_default();
        assert!(
            !reply.trim().is_empty(),
            "assistant_persisted reply should be non-empty"
        );

        bridge.disconnect();
    });
}

/// ST-S17-18：同会话连续两次真实 turn，turn_id 均为合法 UUID 且互不相同。
#[test]
fn st_s17_18_turn_id_unique() {
    case("ST-S17-18", || {
        let config = temp_agent_config("unique");
        let mut bridge = AgentBridge::new(config);

        bridge.connect().expect("connect");

        // 第一次调用（真实 LLM，超时对齐提案 120s）
        let (turn_id_1, out_1) = bridge.submit_and_wait("回复 1", 120).expect("first call");

        // 第二次调用
        let (turn_id_2, out_2) = bridge.submit_and_wait("回复 2", 120).expect("second call");

        // 两个 turn_id 均为合法 UUID 且互不相同；两次均为 turn/completed
        assert_ne!(turn_id_1, turn_id_2, "turn_ids should be unique");
        assert!(is_valid_uuid(&turn_id_1));
        assert!(is_valid_uuid(&turn_id_2));
        for (i, out) in [&out_1, &out_2].iter().enumerate() {
            assert!(
                out.completed.get("session_result").is_some()
                    || out.completed.get("tokens_in").is_some(),
                "call {} should be turn/completed, got: {:?}",
                i + 1,
                out.completed
            );
        }

        bridge.disconnect();
    });
}

/// UT-S17-17：源码静态检查——crates/research-desktop/src 不得存在 DemoTask / DemoAction。
#[test]
fn ut_s17_17_no_demo_task_path() {
    case("UT-S17-17", || {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits = Vec::new();
        for entry in std::fs::read_dir(&src).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read source file");
            for (line_no, line) in text.lines().enumerate() {
                if line.contains("DemoTask") || line.contains("DemoAction") {
                    hits.push(format!("{}:{}: {}", path.display(), line_no + 1, line.trim()));
                }
            }
        }
        assert!(hits.is_empty(), "demo_task 路径残留：\n{}", hits.join("\n"));

        // 编译期断言：AgentBridge 可独立构造，不依赖 demo_task
        let config = temp_agent_config("no-demo");
        let bridge = AgentBridge::new(config);
        assert!(!bridge.is_connected());
    });
}
