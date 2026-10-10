//! 系统动态效果偏好：启动时读取，未知环境默认静止。

/// 将系统“启用动画”值转换为减少动态效果偏好，未知值默认减少。
pub fn reduce_from_enabled(enabled: Option<bool>) -> bool {
    !enabled.unwrap_or(false)
}

/// 任务指示器相位；静止模式不依赖时间，后台轮询由原任务逻辑负责。
pub fn task_phase(reduce: bool, time: f64) -> f32 {
    if reduce { 0.0 } else { time as f32 * 6.0 }
}

/// 原生入口调用一次；测试直接注入偏好，不读取宿主系统。
pub fn system_reduce_motion() -> bool {
    reduce_from_enabled(system_animations_enabled())
}

#[cfg(target_os = "windows")]
fn system_animations_enabled() -> Option<bool> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, value: *mut i32, flags: u32) -> i32;
    }
    let mut enabled = 0_i32;
    // SAFETY：SPI_GETCLIENTAREAANIMATION 接收有效 BOOL 指针，无所有权转移。
    let ok = unsafe { SystemParametersInfoW(0x1042, 0, &mut enabled, 0) };
    (ok != 0).then_some(enabled != 0)
}

#[cfg(target_os = "linux")]
fn system_animations_enabled() -> Option<bool> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let mut child = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "enable-animations"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_millis(300);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    match std::str::from_utf8(&output.stdout).ok()?.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn system_animations_enabled() -> Option<bool> {
    None
}
