//! Launch at login (registry Run key, LaunchAgent or XDG autostart entry).

fn launcher() -> Result<auto_launch::AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name("capshot")
        .set_app_path(&exe.to_string_lossy())
        .build()
        .map_err(|e| e.to_string())
}

pub fn is_enabled() -> bool {
    launcher().and_then(|l| l.is_enabled().map_err(|e| e.to_string())).unwrap_or(false)
}

/// Enable or disable. Enabling again refreshes the stored path, so moving the
/// executable keeps autostart working (winshot's SyncStartupPath).
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let l = launcher()?;
    if enabled {
        l.enable().map_err(|e| e.to_string())
    } else if l.is_enabled().unwrap_or(false) {
        l.disable().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
