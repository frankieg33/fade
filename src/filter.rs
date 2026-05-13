//! Window filtering logic — determines which windows are "system" windows
//! that should be auto-ignored vs. user application windows.

/// Known system process names that should always be filtered out.
const SYSTEM_PROCESSES: &[&str] = &[
    "explorer.exe", // desktop shell (not File Explorer windows — see below)
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "systemsettings.exe",
    "lockapp.exe",
    "runtimebroker.exe",
    "applicationframehost.exe", // UWP frame — the actual app is a child
    "dwm.exe",
    "csrss.exe",
    "winlogon.exe",
    "taskhostw.exe",
    "sihost.exe",
    "ctfmon.exe",
    "conhost.exe",
    "fontdrvhost.exe",
    "dllhost.exe",
    "svchost.exe",
    "smartscreen.exe",
    "securityhealthsystray.exe",
    "windowsterminal.exe", // usually desired — remove if users want to manage it
];

/// Window class names that indicate system/shell windows.
const SYSTEM_CLASSES: &[&str] = &[
    "Progman",                // desktop
    "WorkerW",                // desktop background
    "Shell_TrayWnd",          // taskbar
    "Shell_SecondaryTrayWnd", // secondary taskbar
    "NotifyIconOverflowWindow",
    "Windows.UI.Core.CoreWindow", // UWP system overlays
    "ForegroundStaging",
    "MultitaskingViewFrame",
    "TaskManagerWindow",
];

/// Descriptor for a window, used by the filter logic.
/// Decoupled from raw Win32 types for testability.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub process_name: String,
    pub title: String,
    pub class_name: String,
    pub is_tool_window: bool, // WS_EX_TOOLWINDOW style
    pub is_owned: bool,       // has an owner window (modal/popup/dialog)
    /// True if this is an owned window AND its owner is currently disabled
    /// (`IsWindowEnabled` returns FALSE). Windows disables the parent only for
    /// true application-modal dialogs (Save As, auth prompts, MessageBox).
    /// Floating helpers (find/replace, color pickers, tool palettes) leave the
    /// owner enabled, so they should not shield the parent from idle actions.
    pub disables_owner: bool,
    /// PID of the owner window when this window's owner is disabled (a real
    /// modal). Used to shield the owner process from idle actions for the
    /// out-of-process modal case (owner and dialog hosted in different
    /// processes, e.g. shell-hosted picker dialogs). `None` when not a modal,
    /// not owned, or the lookup failed.
    pub owner_pid: Option<u32>,
    pub own_pid: bool, // belongs to the fade process
    /// True if DWM reports the window as cloaked (hidden by shell/UWP/virtual
    /// desktop). Cloaked windows are invisible to the user even though
    /// IsWindowVisible() returns true, so we must skip them.
    pub is_cloaked: bool,
    /// True if the window belongs to the user's current virtual desktop.
    /// Virtual-desktop resolution requires COM and may fail; default true
    /// when the query cannot be made so we don't drop legitimate windows.
    pub is_on_current_desktop: bool,
}

/// Returns true if this window should be filtered out (is a system window).
pub fn is_system_window(info: &WindowInfo) -> bool {
    // Always filter our own windows
    if info.own_pid {
        return true;
    }

    // Cloaked windows are invisible to the user (UWP background, virtual-desktop
    // residue, shell-hidden surfaces). Acting on them is always wrong.
    if info.is_cloaked {
        return true;
    }

    // Windows on other virtual desktops must not be touched — the user isn't
    // looking at them and idle accounting on the active desktop doesn't apply.
    if !info.is_on_current_desktop {
        return true;
    }

    // Empty title — not a real user window
    if info.title.is_empty() {
        return true;
    }

    // Tool windows (tooltips, floating toolbars)
    if info.is_tool_window {
        return true;
    }

    // Known system process names
    let process_lower = info.process_name.to_lowercase();
    if SYSTEM_PROCESSES.iter().any(|&p| p == process_lower) {
        // Exception: explorer.exe with a non-empty title that isn't the desktop
        // could be a File Explorer window. Check class name.
        if process_lower == "explorer.exe" && info.class_name == "CabinetWClass" {
            return false; // File Explorer folder window — keep it
        }
        return true;
    }

    // Known system window classes
    if SYSTEM_CLASSES.iter().any(|&c| c == info.class_name) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window(process: &str, title: &str, class: &str) -> WindowInfo {
        WindowInfo {
            process_name: process.into(),
            title: title.into(),
            class_name: class.into(),
            is_tool_window: false,
            is_owned: false,
            disables_owner: false,
            owner_pid: None,
            own_pid: false,
            is_cloaked: false,
            is_on_current_desktop: true,
        }
    }

    #[test]
    fn test_cloaked_window_filtered() {
        let mut w = make_window("chrome.exe", "Background tab", "Chrome_WidgetWin_1");
        w.is_cloaked = true;
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_off_desktop_window_filtered() {
        let mut w = make_window("chrome.exe", "Other desktop", "Chrome_WidgetWin_1");
        w.is_on_current_desktop = false;
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_normal_app_passes() {
        let w = make_window("chrome.exe", "Google", "Chrome_WidgetWin_1");
        assert!(!is_system_window(&w));
    }

    #[test]
    fn test_system_process_filtered() {
        let w = make_window("dwm.exe", "Desktop Window Manager", "DWM");
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_empty_title_filtered() {
        let w = make_window("chrome.exe", "", "Chrome_WidgetWin_1");
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_tool_window_filtered() {
        let mut w = make_window("app.exe", "Tooltip", "ToolWin");
        w.is_tool_window = true;
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_own_pid_filtered() {
        let mut w = make_window("fade.exe", "Fade Settings", "SlintWindow");
        w.own_pid = true;
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_system_class_filtered() {
        let w = make_window("unknown.exe", "Something", "Shell_TrayWnd");
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_explorer_desktop_filtered() {
        let w = make_window("explorer.exe", "Desktop", "Progman");
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_explorer_file_window_passes() {
        let w = make_window("explorer.exe", "Documents", "CabinetWClass");
        assert!(!is_system_window(&w));
    }

    #[test]
    fn test_case_insensitive_system_check() {
        let w = make_window("DWM.exe", "Desktop Window Manager", "DWM");
        // Our system list is lowercase, and we compare lowercase
        assert!(is_system_window(&w));
    }

    #[test]
    fn test_windowsterminal_filtered() {
        let w = make_window(
            "WindowsTerminal.exe",
            "Terminal",
            "CASCADIA_HOSTING_WINDOW_CLASS",
        );
        assert!(is_system_window(&w));
    }
}
