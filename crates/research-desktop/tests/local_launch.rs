//! 本地启动脚本静态检查（local-launch-logs-20261012）。
//!
//! UT-S17-31：scripts/run-desktop.sh 存在且可执行、bash -n 语法有效、
//! 含 WSLg 修正 / logs 落盘 / libxkbcommon-x11 预检关键逻辑。
//! 用例 ID 对齐 logos/resources/test/core-S17-test-cases.md。

use nautilus_research_testkit::case;

/// UT-S17-31：启动脚本静态检查（无 LLM、无显示依赖）。
#[test]
fn ut_s17_31_run_desktop_script_static() {
    case("UT-S17-31", || {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/run-desktop.sh");

        // 存在且可执行
        assert!(script.exists(), "脚本应存在：{}", script.display());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&script).expect("metadata").permissions().mode();
            assert!(mode & 0o111 != 0, "脚本应有执行位：{mode:o}");
        }

        // bash -n 语法校验
        let out = std::process::Command::new("bash")
            .arg("-n")
            .arg(&script)
            .output()
            .expect("运行 bash -n");
        assert!(
            out.status.success(),
            "bash -n 语法校验应通过：{}",
            String::from_utf8_lossy(&out.stderr)
        );

        // 关键逻辑静态断言
        let text = std::fs::read_to_string(&script).expect("读脚本");
        for needle in [
            "XDG_RUNTIME_DIR=/mnt/wslg/runtime-dir",
            "unset WAYLAND_DISPLAY",
            "libxkbcommon-x11",
            "LOG_DIR",
            "mkdir -p",
            "tee ",
            "nautilus-research-desktop",
        ] {
            assert!(text.contains(needle), "脚本缺少关键逻辑：{needle}");
        }
    });
}
