/// Windows API wrappers for window enumeration, foreground detection, and window actions.
/// All unsafe Win32 calls are isolated in this module.
///
/// A `WindowApi` trait abstracts these calls for testability.

use crate::filter::WindowInfo;

/// Abstraction over Win32 window operations, enabling mock implementations for tests.
pub trait WindowApi: Send + Sync {
    /// Get the process name of the currently foreground window.
    fn get_foreground_process(&self) -> Option<String>;

    /// Enumerate all visible, non-minimized, top-level windows.
    /// Returns WindowEntry structs with HWND, process name, title, class, and style flags.
    fn enumerate_visible_windows(&self) -> Vec<WindowEntry>;

    /// Minimize a window instantly (no animation).
    fn minimize_window(&self, hwnd: isize);

    /// Close a window by posting WM_CLOSE.
    fn close_window(&self, hwnd: isize);

    /// Check if a window is fullscreen.
    fn is_fullscreen(&self, hwnd: isize) -> bool;

    /// Check if a window still exists.
    fn is_window_valid(&self, hwnd: isize) -> bool;
}

/// Descriptor pairing a WindowInfo with its HWND for action dispatch.
#[derive(Debug, Clone)]
pub struct WindowEntry {
    pub hwnd: isize,
    pub info: WindowInfo,
}

/// Real Win32 implementation.
#[cfg(target_os = "windows")]
pub struct Win32Api {
    own_pid: u32,
}

#[cfg(target_os = "windows")]
impl Win32Api {
    pub fn new() -> Self {
        Self {
            own_pid: std::process::id(),
        }
    }
}

#[cfg(target_os = "windows")]
mod win32_impl {
    use super::*;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::PWSTR;

    impl WindowApi for Win32Api {
        fn get_foreground_process(&self) -> Option<String> {
            unsafe {
                let hwnd = GetForegroundWindow();
                if hwnd.0.is_null() {
                    return None;
                }
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == 0 {
                    return None;
                }
                get_process_name_from_pid(pid)
            }
        }

        fn enumerate_visible_windows(&self) -> Vec<WindowEntry> {
            let own_pid = self.own_pid;
            unsafe {
                let mut ctx = EnumContext {
                    results: Vec::new(),
                    own_pid,
                };
                let _ = EnumWindows(
                    Some(enum_window_callback_v2),
                    LPARAM(&mut ctx as *mut EnumContext as isize),
                );
                ctx.results
            }
        }

        fn minimize_window(&self, hwnd: isize) {
            unsafe {
                let h = HWND(hwnd as *mut _);
                if !IsWindow(h).as_bool() {
                    return;
                }
                // Use SetWindowPlacement for instant minimize (bypasses animation)
                let mut placement = WINDOWPLACEMENT {
                    length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                    ..Default::default()
                };
                if GetWindowPlacement(h, &mut placement).is_ok() {
                    placement.showCmd = SW_SHOWMINIMIZED.0 as u32;
                    let _ = SetWindowPlacement(h, &placement);
                }
            }
        }

        fn close_window(&self, hwnd: isize) {
            unsafe {
                let h = HWND(hwnd as *mut _);
                if !IsWindow(h).as_bool() {
                    return;
                }
                let _ = PostMessageW(h, WM_CLOSE, None, None);
            }
        }

        fn is_fullscreen(&self, hwnd: isize) -> bool {
            unsafe {
                let h = HWND(hwnd as *mut _);
                if !IsWindow(h).as_bool() {
                    return false;
                }

                let style = GetWindowLongW(h, GWL_STYLE) as u32;
                let _ex_style = GetWindowLongW(h, GWL_EXSTYLE) as u32;

                // Fullscreen windows are typically WS_POPUP without WS_THICKFRAME
                let is_popup = (style & WS_POPUP.0) != 0;
                let no_border = (style & WS_THICKFRAME.0) == 0;

                if !(is_popup && no_border) {
                    return false;
                }

                // Check if window covers the entire monitor
                let mut rect = std::mem::zeroed::<windows::Win32::Foundation::RECT>();
                let _ = GetWindowRect(h, &mut rect);

                let monitor = MonitorFromWindow(h, MONITOR_DEFAULTTOPRIMARY);
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                    rect.left == mi.rcMonitor.left
                        && rect.top == mi.rcMonitor.top
                        && rect.right == mi.rcMonitor.right
                        && rect.bottom == mi.rcMonitor.bottom
                } else {
                    false
                }
            }
        }

        fn is_window_valid(&self, hwnd: isize) -> bool {
            unsafe { IsWindow(HWND(hwnd as *mut _)).as_bool() }
        }
    }

    struct EnumContext {
        results: Vec<WindowEntry>,
        own_pid: u32,
    }

    unsafe extern "system" fn enum_window_callback_v2(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumContext);

        // Skip invisible windows
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }

        // Skip minimized windows
        if IsIconic(hwnd).as_bool() {
            return TRUE;
        }

        // Get window title
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        if len == 0 {
            return TRUE; // no title
        }
        let title = String::from_utf16_lossy(&title_buf[..len as usize]);

        // Get window class
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);

        // Get PID
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        // Get process name
        let process_name = match get_process_name_from_pid(pid) {
            Some(name) => name,
            None => return TRUE, // can't identify — skip
        };

        // Check extended style
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let is_tool_window = (ex_style & WS_EX_TOOLWINDOW.0) != 0;

        // Check if owned
        let owner = GetWindow(hwnd, GW_OWNER);
        let is_owned = owner.is_ok() && !owner.unwrap().0.is_null();

        let info = WindowInfo {
            process_name,
            title,
            class_name,
            is_tool_window,
            is_owned,
            own_pid: pid == ctx.own_pid,
        };

        ctx.results.push(WindowEntry {
            hwnd: hwnd.0 as isize,
            info,
        });

        TRUE
    }

    pub(super) unsafe fn get_process_name_from_pid(pid: u32) -> Option<String> {
        use windows::Win32::Foundation::CloseHandle;
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let pwstr = PWSTR(buf.as_mut_ptr());
        let result = if QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), pwstr, &mut size).is_ok() {
            let full_path = String::from_utf16_lossy(&buf[..size as usize]);
            full_path.rsplit('\\').next().map(|s| s.to_string())
        } else {
            None
        };
        let _ = CloseHandle(handle);
        result
    }
}

/// Handle guard for the Win32 foreground event hook. Unhooks on drop.
#[cfg(target_os = "windows")]
pub struct ForegroundHookGuard {
    handle: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
}

#[cfg(target_os = "windows")]
impl Drop for ForegroundHookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::UI::Accessibility::UnhookWinEvent(self.handle);
        }
        log::info!("Foreground event hook uninstalled");
    }
}

/// Install a Win32 event hook that updates shared foreground timestamps
/// whenever the foreground window changes.
///
/// MUST be called from a thread with a Win32 message pump (the Slint UI thread).
/// Returns a guard that unhooks on drop.
#[cfg(target_os = "windows")]
pub fn install_foreground_hook(
    timestamps: crate::monitor::ForegroundTimestamps,
) -> Result<ForegroundHookGuard, String> {
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, GetWindowThreadProcessId, WINEVENT_OUTOFCONTEXT,
    };

    // Store the shared timestamps in a global so the callback can access them.
    // Safe because install_foreground_hook is called once from the main thread.
    static HOOK_TIMESTAMPS: OnceLock<crate::monitor::ForegroundTimestamps> = OnceLock::new();
    HOOK_TIMESTAMPS
        .set(timestamps)
        .map_err(|_| "Foreground hook already installed".to_string())?;

    unsafe extern "system" fn hook_callback(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        if hwnd.0.is_null() {
            return;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return;
        }
        // Resolve PID to process name
        if let Some(name) = win32_impl::get_process_name_from_pid(pid) {
            if let Some(ts) = HOOK_TIMESTAMPS.get() {
                if let Ok(mut map) = ts.lock() {
                    map.insert(name.to_lowercase(), std::time::Instant::now());
                }
            }
        }
    }

    let handle = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None, // no DLL — WINEVENT_OUTOFCONTEXT
            Some(hook_callback),
            0, // all processes
            0, // all threads
            WINEVENT_OUTOFCONTEXT,
        )
    };

    if handle.0.is_null() {
        return Err("SetWinEventHook returned null".to_string());
    }

    log::info!("Foreground event hook installed");
    Ok(ForegroundHookGuard { handle })
}

/// Stub implementation for non-Windows platforms (for compilation/testing).
#[cfg(not(target_os = "windows"))]
pub struct Win32Api;

#[cfg(not(target_os = "windows"))]
impl Win32Api {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(not(target_os = "windows"))]
impl WindowApi for Win32Api {
    fn get_foreground_process(&self) -> Option<String> {
        None
    }

    fn enumerate_visible_windows(&self) -> Vec<WindowEntry> {
        Vec::new()
    }

    fn minimize_window(&self, _hwnd: isize) {}
    fn close_window(&self, _hwnd: isize) {}
    fn is_fullscreen(&self, _hwnd: isize) -> bool {
        false
    }
    fn is_window_valid(&self, _hwnd: isize) -> bool {
        false
    }
}

#[cfg(not(target_os = "windows"))]
pub struct ForegroundHookGuard;

#[cfg(not(target_os = "windows"))]
pub fn install_foreground_hook(
    _timestamps: crate::monitor::ForegroundTimestamps,
) -> Result<ForegroundHookGuard, String> {
    Ok(ForegroundHookGuard)
}

/// Mock implementation for tests.
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    pub struct MockWindowApi {
        pub foreground_process: Arc<Mutex<Option<String>>>,
        pub windows: Arc<Mutex<Vec<WindowEntry>>>,
        pub minimized: Arc<Mutex<Vec<isize>>>,
        pub closed: Arc<Mutex<Vec<isize>>>,
        pub fullscreen_hwnds: Arc<Mutex<Vec<isize>>>,
        pub valid_hwnds: Arc<Mutex<Vec<isize>>>,
    }

    #[allow(dead_code)]
    impl MockWindowApi {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn set_foreground(&self, process: Option<&str>) {
            *self.foreground_process.lock().unwrap() = process.map(|s| s.to_string());
        }

        pub fn set_windows(&self, entries: Vec<WindowEntry>) {
            let hwnds: Vec<isize> = entries.iter().map(|e| e.hwnd).collect();
            *self.windows.lock().unwrap() = entries;
            *self.valid_hwnds.lock().unwrap() = hwnds;
        }

        pub fn get_minimized(&self) -> Vec<isize> {
            self.minimized.lock().unwrap().clone()
        }

        pub fn get_closed(&self) -> Vec<isize> {
            self.closed.lock().unwrap().clone()
        }

        pub fn set_fullscreen(&self, hwnd: isize) {
            self.fullscreen_hwnds.lock().unwrap().push(hwnd);
        }

        pub fn invalidate_window(&self, hwnd: isize) {
            self.valid_hwnds.lock().unwrap().retain(|&h| h != hwnd);
        }
    }

    impl WindowApi for MockWindowApi {
        fn get_foreground_process(&self) -> Option<String> {
            self.foreground_process.lock().unwrap().clone()
        }

        fn enumerate_visible_windows(&self) -> Vec<WindowEntry> {
            self.windows.lock().unwrap().clone()
        }

        fn minimize_window(&self, hwnd: isize) {
            self.minimized.lock().unwrap().push(hwnd);
        }

        fn close_window(&self, hwnd: isize) {
            self.closed.lock().unwrap().push(hwnd);
        }

        fn is_fullscreen(&self, hwnd: isize) -> bool {
            self.fullscreen_hwnds.lock().unwrap().contains(&hwnd)
        }

        fn is_window_valid(&self, hwnd: isize) -> bool {
            self.valid_hwnds.lock().unwrap().contains(&hwnd)
        }
    }
}
