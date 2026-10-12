//! S22 场景测试（online-chat-llm-config-20261012）：真实 LLM / 真实 octos。
//!
//! - ST-S22-01：自由问答经真实 LLM turn 回复（非离线预设兜底文案）。
//! - ST-S22-02：provider 配置闭环 catalog → upsert → test → list
//!   （临时实例数据目录隔离，不动用户真实 profile；api_key 仅内存传输）。
//!
//! 注意：调用真实 LLM 会产生 API 调用；profile 选择遵循 RESEARCH_OCTOS_PROFILE
//! 环境变量，缺省回退 ~/.octos/profiles/octos.json（与 desktop 口径一致）。

#[path = "support/profile.rs"]
mod test_profile;

use nautilus_research_desktop::agent_bridge::{AgentBridge, AgentSessionConfig};
use nautilus_research_desktop::oup::{
    LlmRouteInput, LlmSelectionInput, OupClient, OupConfig,
};
use nautilus_research_testkit::case;

/// 临时实例目录（数据 + 实例隔离；复制真实 profile 供 upsert 落盘）。
struct TempDirs {
    base: std::path::PathBuf,
    data_dir: std::path::PathBuf,
    instance_dir: std::path::PathBuf,
}

impl TempDirs {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!(
            "rd-s22-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let data_dir = base.join("data");
        let instance_dir = base.join("instance");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&instance_dir).expect("create instance dir");
        // 复制真实 LLM profile（issue #24：经 HOME/RESEARCH_OCTOS_PROFILE 解析）
        if let Some(src) = test_profile::configured_source() {
            let dst_dir = data_dir.join("profiles");
            std::fs::create_dir_all(&dst_dir).expect("create profiles dir");
            std::fs::copy(src, dst_dir.join("octos.json")).expect("copy profile");
        }
        Self {
            base,
            data_dir,
            instance_dir,
        }
    }

    fn oup_config(&self) -> OupConfig {
        OupConfig {
            octos_bin: "octos".into(),
            data_dir: self.data_dir.to_string_lossy().into(),
            instance_dir: self.instance_dir.to_string_lossy().into(),
            // issue #24：禁止机器绝对路径；仓库根经 CARGO_MANIFEST_DIR 解析
            workspace_root: concat!(env!("CARGO_MANIFEST_DIR"), "/../..").into(),
            extra_env: Vec::new(),
            danger_full_access: false,
        }
    }
}

impl Drop for TempDirs {
    fn drop(&mut self) {
        // 用例收尾：临时实例目录整体删除（含 upsert 写入的 profile 副本）
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// ST-S22-01：真实 LLM 自由对话——发送通用问题（非业务意图），
/// turn/completed 终态且回复非空、不含离线预设兜底文案。
#[test]
fn st_s22_01_real_llm_free_chat() {
    case("ST-S22-01", || {
        let dirs = TempDirs::new("chat");
        let config = AgentSessionConfig {
            session_id: "s22-chat".into(),
            profile_id: "octos".into(),
            profile_warning: None,
            oup: dirs.oup_config(),
        };
        let mut bridge = AgentBridge::new(config);
        bridge.connect().expect("connect to octos serve");

        // 自由问答（非业务意图）：复用 ai::chat_prompt 线上模板
        let prompt = nautilus_research_desktop::ai::chat_prompt(
            "S22 验收项目",
            "（尚未确认需求版本）",
            "（尚未保存设计版本）",
            "用一句话解释什么是移动平均策略",
        );
        let (_turn_id, outcome) = bridge
            .submit_and_wait(&prompt, 420)
            .expect("free chat turn should complete");

        let reply = outcome
            .assistant_text
            .unwrap_or_default();
        assert!(!reply.trim().is_empty(), "助手回复应非空");
        // 不含离线预设兜底文案（ai::reply Unknown 分支特征句）
        assert!(
            !reply.contains("不能理解任意指令") && !reply.contains("离线预设对话"),
            "回复不得为离线预设兜底文案，实际：{}",
            reply.chars().take(120).collect::<String>()
        );
    });
}

/// ST-S22-02：provider 配置闭环（临时实例目录隔离）——
/// catalog 非空 → upsert（复用当前 primary 的 route，api_key 缺省经环境变量）
/// → test 连通 → list 可见且 set_primary 生效。
#[test]
fn st_s22_02_llm_provider_config_loop() {
    case("ST-S22-02", || {
        let dirs = TempDirs::new("cfg");
        let mut client = OupClient::spawn(&dirs.oup_config()).expect("spawn octos");
        client
            .session_open_with_profile("s22-cfg", "octos")
            .expect("session open");

        // 1. catalog：非空且含已知 family
        let catalog = client.llm_catalog().expect("llm_catalog");
        assert!(!catalog.is_empty(), "catalog 应非空");

        // 2. list：取当前 primary 作为复用基准（profile 副本内已配置）
        let before = client.llm_list().expect("llm_list before");
        let primary = before
            .primary
            .clone()
            .expect("profile 副本应已有主模型配置");
        let fam = primary.family_id.clone();
        assert!(
            catalog.iter().any(|f| f.family_id == fam),
            "catalog 应含当前 primary 的 family：{fam}"
        );

        // 3. upsert：同 family/model、新 label、复用 route；api_key 缺省
        //    （服务端从 profile env_vars 解析；本用例不经内存传 key）
        let selection = LlmSelectionInput {
            family_id: fam.clone(),
            model_id: primary.model_id.clone(),
            route: LlmRouteInput {
                route_id: String::new(),
                label: Some("st-s22-02 验收".into()),
                base_url: primary.base_url.clone(),
                api_key_env: primary.api_key_env.clone(),
                api_type: None,
            },
        };
        let upsert = client
            .llm_upsert(&selection, None, true)
            .expect("llm_upsert");
        assert_eq!(
            upsert.get("applied").and_then(|v| v.as_bool()),
            Some(true),
            "upsert 应返回 applied=true，实际：{upsert}"
        );

        // 4. test：连通成功（契约：applied=true + message="Provider connection
        //    verified"；key 由服务端经环境变量解析）
        let test = client.llm_test(&selection, None).expect("llm_test");
        assert_eq!(
            test.get("applied").and_then(|v| v.as_bool()),
            Some(true),
            "连通测试应 applied=true，实际响应={test}"
        );
        assert!(
            test.get("error").is_none(),
            "连通成功不应带 error 字段，实际响应={test}"
        );

        // 5. list：新配置可见且 set_primary 生效
        let after = client.llm_list().expect("llm_list after");
        let now = after.primary.expect("upsert 后应有主模型");
        assert_eq!(now.family_id, fam);
        assert_eq!(now.model_id, primary.model_id);
        assert_eq!(
            now.label.as_deref(),
            Some("st-s22-02 验收"),
            "list 应反映新 label"
        );
        assert!(now.has_api_key, "环境变量 key 应被服务端判定为已配置");
    });
}
