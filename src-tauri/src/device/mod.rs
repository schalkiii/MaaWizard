use maa_framework::toolkit::Toolkit;
use serde::Serialize;

/// 一个可被 Win32 控制器驱动的桌面窗口
#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    pub hwnd: usize,
    pub class_name: String,
    pub window_name: String,
}

/// 列举当前可见的桌面窗口（仅 Windows）。委托给 MaaFramework 的 Toolkit，
/// 避免自行实现 Win32 枚举（见 spec：设备控制统一由 M0 负责）。
pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
    let windows = Toolkit::find_desktop_windows().map_err(|e| e.to_string())?;

    // 无标题窗口通常无法自动化，过滤掉以减少干扰
    Ok(windows
        .into_iter()
        .filter(|window| !window.window_name.trim().is_empty())
        .map(|window| WindowInfo {
            hwnd: window.hwnd,
            class_name: window.class_name,
            window_name: window.window_name,
        })
        .collect())
}
