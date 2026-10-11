//! 验收配置来源与桌面既有选择规则一致，不改写用户配置。
use std::path::PathBuf;

pub(crate) fn source(explicit: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    explicit.filter(|p| p.is_file()).or_else(|| {
        home.map(|h| h.join(".octos/profiles/octos.json"))
            .filter(|p| p.is_file())
    })
}

pub(crate) fn configured_source() -> Option<PathBuf> {
    source(
        std::env::var_os("RESEARCH_OCTOS_PROFILE").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}
