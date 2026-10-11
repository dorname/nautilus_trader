//! octos server 二进制保障（backend ensure）：三级解析 + 版本 pin。
//!
//! 参考 octoscode `backend_ensure.rs` 的客户端实践（embed-octos-runtime 提案）：
//! 桌面 spawn 前先解析 octos 二进制并校验协议版本，不裸依赖用户 PATH。
//!
//! 解析顺序（逐级降级，命中即用，启动日志记录实际来源）：
//! 1. **分发内嵌**：`RESEARCH_OCTOS_EMBED_DIR` 指定目录（打包填充）或
//!    可执行文件同级 `octos-bin/octos`（随包 bundle，对齐 core-03「运行时
//!    随包、不依赖用户 PATH」的 Python 桥接先例）；内嵌来源视为已 pin，跳过
//!    版本校验。
//! 2. **octoscode 兼容安装位**：`$HOME/.octos/bin/octos`。
//! 3. **PATH**：交给 OS 查找（spawn `--version` 探测）。
//!
//! 非内嵌来源执行 `octos --version` 与 [`REQUIRED_OCTOS_RELEASE`] 严格比对；
//! 不匹配/缺失返回中文诊断（embed-octos-runtime：不静默继续）。
//! 测试逃生门：`RESEARCH_OCTOS_SKIP_ENSURE=1` 跳过校验（仅限无 octos 的
//! 纯编译环境，产品路径不得使用）。

use std::path::PathBuf;

/// 客户端编译期 pin 的 octos release（与分发物内嵌 bundle 一致；
/// 协议类型若引入 octos-core，其 git rev 必须对应同一 release）。
pub const REQUIRED_OCTOS_RELEASE: &str = "2.0.3-rc.9";

/// 逃生门环境变量：值为 `1` 时跳过解析与版本校验（测试专用）。
const SKIP_ENSURE_ENV: &str = "RESEARCH_OCTOS_SKIP_ENSURE";

/// 分发内嵌目录环境变量（打包脚本填充；优先于 exe 同级探测）。
const EMBED_DIR_ENV: &str = "RESEARCH_OCTOS_EMBED_DIR";

/// 解析结果：实际使用的二进制与来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedOctos {
    /// octos 二进制路径（PATH 来源时为相对名，spawn 交给 OS 解析）。
    pub bin: String,
    /// 命中的来源（内嵌来源跳过版本校验）。
    pub source: OctosSource,
}

/// octos 二进制来源（三级解析的命中档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OctosSource {
    /// 分发内嵌（exe 同级 octos-bin/ 或 RESEARCH_OCTOS_EMBED_DIR）。
    Embedded,
    /// `~/.octos/bin`（octoscode 兼容安装位）。
    OctosHome,
    /// 用户 PATH。
    Path,
}

impl OctosSource {
    /// 启动日志用的来源标签。
    pub fn label(self) -> &'static str {
        match self {
            OctosSource::Embedded => "embedded",
            OctosSource::OctosHome => "~/.octos/bin",
            OctosSource::Path => "PATH",
        }
    }
}

/// 保障失败：缺失或版本不匹配（均带中文引导，不静默继续）。
#[derive(Debug)]
pub enum OctosEnsureError {
    /// 三级来源均不可用。
    NotFound {
        /// 中文引导文案（含探测过的位置与安装指引）。
        guidance: String,
    },
    /// 版本与 pin 不符（含实际输出与期望值）。
    VersionMismatch {
        /// 探测到的 octos 版本串（`--version` 输出首个空格后的 token）。
        found: String,
        /// 编译期 pin 的版本。
        expected: String,
        /// 命中的二进制（诊断用）。
        bin: String,
    },
}

impl std::fmt::Display for OctosEnsureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OctosEnsureError::NotFound { guidance } => {
                write!(f, "未找到 octos 二进制：{guidance}")
            }
            OctosEnsureError::VersionMismatch {
                found,
                expected,
                bin,
            } => write!(
                f,
                "octos 版本不匹配（{bin} 为 {found}，本程序需要 {expected}）：\
                 协议可能不兼容，请安装匹配版本或使用分发物内嵌的 octos"
            ),
        }
    }
}

impl std::error::Error for OctosEnsureError {}

impl From<OctosEnsureError> for crate::oup::OupError {
    fn from(e: OctosEnsureError) -> Self {
        // 保障失败归一到 Spawn 通路：OupClient::spawn 前置校验的统一出口。
        crate::oup::OupError::Spawn(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            e.to_string(),
        ))
    }
}

/// 解析并校验 octos 二进制（spawn 前调用；逃生门直通 PATH）。
pub fn ensure() -> Result<ResolvedOctos, OctosEnsureError> {
    if skip_requested(std::env::var_os(SKIP_ENSURE_ENV).as_deref()) {
        return Ok(ResolvedOctos {
            bin: "octos".into(),
            source: OctosSource::Path,
        });
    }
    // ① 分发内嵌
    if let Some(bin) = embedded_bin() {
        return Ok(ResolvedOctos {
            bin,
            source: OctosSource::Embedded,
        });
    }
    // ② ~/.octos/bin（octoscode 兼容安装位）：探测失败（如依赖缺失无法执行）
    // 降级续试 PATH，不直接 NotFound；版本明确不匹配才报 VersionMismatch。
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home).join(".octos/bin/octos");
        if let Some(bin) = usable(&candidate) {
            match probe_version(&bin) {
                Some(found) if found == REQUIRED_OCTOS_RELEASE => {
                    return Ok(ResolvedOctos {
                        bin,
                        source: OctosSource::OctosHome,
                    });
                }
                Some(found) => {
                    return Err(OctosEnsureError::VersionMismatch {
                        found,
                        expected: REQUIRED_OCTOS_RELEASE.into(),
                        bin,
                    });
                }
                None => { /* 无法执行（如 GLIBC 不满足）：降级试 PATH */ }
            }
        }
    }
    // ③ PATH（spawn --version 探测，由 OS 解析）
    match probe_version("octos") {
        Some(found) if found == REQUIRED_OCTOS_RELEASE => Ok(ResolvedOctos {
            bin: "octos".into(),
            source: OctosSource::Path,
        }),
        Some(found) => Err(OctosEnsureError::VersionMismatch {
            found,
            expected: REQUIRED_OCTOS_RELEASE.into(),
            bin: "octos".into(),
        }),
        None => Err(OctosEnsureError::NotFound {
            guidance: guidance_text(),
        }),
    }
}

/// 运行 `<bin> --version` 并解析版本 token（形如
/// `octos 2.0.3-rc.9 (hash date)`，取首个空格后的 token）。
/// 退出码非 0（如 GLIBC 不满足、依赖缺失）或无有效输出 → None
/// （调用方降级续试下一级或汇总 NotFound；不得把报错文本当版本）。
fn probe_version(bin: &str) -> Option<String> {
    let output = std::process::Command::new(bin).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.split_whitespace()
        .nth(1)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

/// 分发内嵌位：RESEARCH_OCTOS_EMBED_DIR 优先，其次 exe 同级 octos-bin/octos。
fn embedded_bin() -> Option<String> {
    if let Some(dir) = std::env::var_os(EMBED_DIR_ENV) {
        if let Some(bin) = usable(&PathBuf::from(dir).join("octos")) {
            return Some(bin);
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("octos-bin/octos")))
        .and_then(|p| usable(&p))
}

/// 路径存在且可执行则返回字符串路径（探测失败静默降级到下一级）。
fn usable(p: &std::path::Path) -> Option<String> {
    let ok = p.is_file()
        && {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                p.metadata()
                    .map(|m| m.permissions().mode() & 0o111 != 0)
                    .unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                true
            }
        };
    ok.then(|| p.to_string_lossy().into_owned())
}

/// 逃生门判定（纯函数，UT 可达）：仅值为 `1` 时跳过（防误配直通）。
fn skip_requested(v: Option<&std::ffi::OsStr>) -> bool {
    v == Some(std::ffi::OsStr::new("1"))
}

/// 中文引导文案：探测过的位置与安装指引（离线交付口径：不自动下载）。
fn guidance_text() -> String {
    "已依次探测：分发内嵌（octos-bin/ 或 RESEARCH_OCTOS_EMBED_DIR）、\
     ~/.octos/bin/octos、PATH。请安装 octos 并确认版本，或使用随包分发的\
     research-desktop（内嵌 octos 运行时）。"
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UT-S17-28：来源标签与逃生门判定（纯函数，不写进程环境）。
    #[test]
    fn source_labels_and_skip_gate() {
        nautilus_research_testkit::case("UT-S17-28", || {
        assert_eq!(OctosSource::Embedded.label(), "embedded");
        assert_eq!(OctosSource::OctosHome.label(), "~/.octos/bin");
        assert_eq!(OctosSource::Path.label(), "PATH");
        // 逃生门：仅值恰好为 1 生效（空/0/其他值不直通，防误配）。
        assert!(skip_requested(Some(std::ffi::OsStr::new("1"))));
        assert!(!skip_requested(Some(std::ffi::OsStr::new("0"))));
        assert!(!skip_requested(Some(std::ffi::OsStr::new(""))));
        assert!(!skip_requested(None));
        });
    }

    /// UT-S17-28：版本不匹配给出中文诊断（含实际/期望版本与二进制）。
    #[test]
    fn version_mismatch_diagnostics() {
        nautilus_research_testkit::case("UT-S17-28b", || {
        let err = OctosEnsureError::VersionMismatch {
            found: "0.0.1".into(),
            expected: REQUIRED_OCTOS_RELEASE.into(),
            bin: "/fake/octos".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("0.0.1") && msg.contains(REQUIRED_OCTOS_RELEASE));
        assert!(msg.contains("/fake/octos"));
        assert!(msg.contains("版本不匹配"));
        });
    }

    /// UT-S17-28：本机 octos 应可用且版本匹配 pin（三级解析端到端）。
    #[test]
    fn ensure_resolves_on_this_machine() {
        nautilus_research_testkit::case("UT-S17-28c", || {
        match ensure() {
            Ok(r) => assert!(!r.bin.is_empty()),
            // 无 octos 的纯编译环境：NotFound 带中文引导（逃生门语义另测）。
            Err(OctosEnsureError::NotFound { guidance }) => {
                assert!(guidance.contains("RESEARCH_OCTOS_EMBED_DIR") || guidance.contains("PATH"))
            }
            Err(e) => panic!("本机 octos 版本应匹配 pin：{e}"),
        }
        });
    }
}
