/// Auto-start management — register/unregister fade to start with Windows.

use auto_launch::AutoLaunch;

pub fn set_auto_start(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;
    let exe_str = exe
        .to_str()
        .ok_or_else(|| "Exe path is not valid UTF-8".to_string())?;

    let auto = AutoLaunch::new("Fade", exe_str, &[] as &[&str]);

    if enabled {
        auto.enable().map_err(|e| format!("Failed to enable auto-start: {}", e))
    } else {
        auto.disable().map_err(|e| format!("Failed to disable auto-start: {}", e))
    }
}

#[allow(dead_code)]
pub fn is_auto_start_enabled() -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let exe_str = match exe.to_str() {
        Some(s) => s,
        None => return false,
    };

    let auto = AutoLaunch::new("Fade", exe_str, &[] as &[&str]);
    auto.is_enabled().unwrap_or(false)
}
