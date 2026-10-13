//! S23 场景测试（fix-issues-38-39-20261013 / issue #39）：真实 LLM 需求草稿生成。
//!
//! - ST-S23-01：以一句自然语言研究想法派发草稿 prompt，turn/completed 终态，
//!   回复非空且含结构化要素，不含离线预设兜底文案。
//!
//! 注意：调用真实 LLM 会产生 API 调用；profile 选择遵循 RESEARCH_OCTOS_PROFILE
//! 环境变量，缺省回退 ~/.octos/profiles/octos.json（与 desktop 口径一致）。

#[path = "support/profile.rs"]
mod test_profile;

use nautilus_research_desktop::agent_bridge::{AgentBridge, AgentSessionConfig};
use nautilus_research_desktop::oup::OupConfig;
use nautilus_research_testkit::case;

/// ST-S23-01：真实 LLM 需求草稿生成——自然语言研究想法 → 结构化草稿。
#[test]
fn st_s23_01_real_llm_requirement_draft() {
    case("ST-S23-01", || {
        // 临时实例目录隔离（同 wire_real_llm 口径）
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!(
            "rd-s23-draft-{}-{nanos}",
            std::process::id()
        ));
        let data_dir = base.join("data");
        let instance_dir = base.join("instance");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&instance_dir).expect("create instance dir");
        if let Some(src) = test_profile::configured_source() {
            let dst_dir = data_dir.join("profiles");
            std::fs::create_dir_all(&dst_dir).expect("create profiles dir");
            std::fs::copy(src, dst_dir.join("octos.json")).expect("copy profile");
        }

        let config = AgentSessionConfig {
            session_id: "s23-draft".into(),
            profile_id: "octos".into(),
            profile_warning: None,
            oup: OupConfig {
                octos_bin: "octos".into(),
                data_dir: data_dir.to_string_lossy().into(),
                instance_dir: instance_dir.to_string_lossy().into(),
                workspace_root: concat!(env!("CARGO_MANIFEST_DIR"), "/../..").into(),
                extra_env: Vec::new(),
                danger_full_access: false,
            },
        };
        let mut bridge = AgentBridge::new(config);
        bridge.connect().expect("connect to octos serve");

        // 复用线上模板（ai::requirement_draft_prompt）
        let prompt = nautilus_research_desktop::ai::requirement_draft_prompt(
            "S23 验收项目",
            "帮我研究一个 A 股双均线策略，5 日和 20 日均线交叉买卖，日线数据",
        );
        let (_turn_id, outcome) = bridge
            .submit_and_wait(&prompt, 420)
            .expect("draft turn should complete");

        let reply = outcome.assistant_text.unwrap_or_default();
        assert!(!reply.trim().is_empty(), "草稿回复应非空");
        // 结构化要素：目标/范围/参数/验收 至少命中三类
        let hits = ["目标", "范围", "参数", "验收"]
            .iter()
            .filter(|k| reply.contains(**k))
            .count();
        assert!(
            hits >= 3,
            "草稿应含结构化要素（目标/范围/参数/验收 至少三类），命中 {hits}，实际：{}",
            reply.chars().take(200).collect::<String>()
        );
        // 不含离线预设兜底文案
        assert!(
            !reply.contains("不能理解任意指令") && !reply.contains("离线预设对话"),
            "草稿不得为离线预设兜底文案"
        );

        let _ = std::fs::remove_dir_all(&base);
    });
}
