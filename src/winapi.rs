/// Windows API wrappers for window enumeration, foreground detection, and window actions.
/// All unsafe Win32 calls are isolated in this module.
///
/// A `WindowApi` trait abstracts these calls for testability.
use crate::filter::WindowInfo;

/// Abstraction over Win32 window operations, enabling mock implementations for tests.
pub trait WindowApi: Send + Sync {
    /// Get the process name of the currently foreground window.
    fn get_foreground_process(&self) -> Option<String>;

    /// Get the HWND of the currently foreground window, or 0 if none.
    /// Used to compare candidate windows by handle rather than process name —
    /// process-name comparison is ambiguous when multiple processes share a name
    /// or when host processes (ApplicationFrameHost, browsers) reparent windows.
    fn get_foreground_hwnd(&self) -> isize {
        0
    }

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

    /// Get the creation time of a process, or None if unavailable
    /// (access denied, process exited, protected process, etc).
    fn process_start_time(&self, pid: u32) -> Option<std::time::SystemTime>;
}

/// Descriptor pairing a WindowInfo with its HWND for action dispatch.
#[derive(Debug, Clone)]
pub struct WindowEntry {
    pub hwnd: isize,
    pub pid: u32,
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
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::*;

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

        fn get_foreground_hwnd(&self) -> isize {
            // SAFETY: GetForegroundWindow has no preconditions; null is valid.
            unsafe { GetForegroundWindow().0 as isize }
        }

        fn enumerate_visible_windows(&self) -> Vec<WindowEntry> {
            let own_pid = self.own_pid;
            // SAFETY: ctx is stack-allocated and outlives the EnumWindows call.
            unsafe {
                let mut ctx = EnumContext {
                    results: Vec::new(),
                    own_pid,
                    pid_name_cache: std::collections::HashMap::new(),
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
                let is_popup = (style & WS_POPUP.0) != 0;
                let no_border = (style & WS_THICKFRAME.0) == 0;
                let classic_fullscreen = is_popup && no_border;

                // Resolve window rect + monitor rect once for both heuristics.
                let mut rect = std::mem::zeroed::<windows::Win32::Foundation::RECT>();
                if GetWindowRect(h, &mut rect).is_err() {
                    return false;
                }
                let monitor = MonitorFromWindow(h, MONITOR_DEFAULTTOPRIMARY);
                let mut mi = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
                    return false;
                }

                // Path 1: classic borderless-popup fullscreen with exact match.
                if classic_fullscreen
                    && rect.left == mi.rcMonitor.left
                    && rect.top == mi.rcMonitor.top
                    && rect.right == mi.rcMonitor.right
                    && rect.bottom == mi.rcMonitor.bottom
                {
                    return true;
                }

                // Path 2: borderless/windowed-fullscreen variants used by DXGI and
                // many games — window may have WS_OVERLAPPEDWINDOW styles but its
                // client area covers (nearly) the whole monitor with no taskbar
                // visible. Treat ≥99% coverage of monitor area as fullscreen,
                // tolerating a few pixels of DWM extended frame.
                let win_w = (rect.right - rect.left).max(0) as i64;
                let win_h = (rect.bottom - rect.top).max(0) as i64;
                let mon_w = (mi.rcMonitor.right - mi.rcMonitor.left).max(1) as i64;
                let mon_h = (mi.rcMonitor.bottom - mi.rcMonitor.top).max(1) as i64;
                let win_area = win_w * win_h;
                let mon_area = mon_w * mon_h;
                // Tolerance: 1% slack on each dimension, plus the window must
                // start at-or-before the monitor origin.
                let near_full = win_area * 100 >= mon_area * 99
                    && rect.left <= mi.rcMonitor.left + 4
                    && rect.top <= mi.rcMonitor.top + 4
                    && rect.right >= mi.rcMonitor.right - 4
                    && rect.bottom >= mi.rcMonitor.bottom - 4;
                near_full
            }
        }

        fn is_window_valid(&self, hwnd: isize) -> bool {
            unsafe { IsWindow(HWND(hwnd as *mut _)).as_bool() }
        }

        fn process_start_time(&self, pid: u32) -> Option<std::time::SystemTime> {
            use windows::Win32::Foundation::{CloseHandle, FILETIME};
            use windows::Win32::System::Threading::GetProcessTimes;
            unsafe {
                let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
                let mut creation = FILETIME::default();
                let mut exit = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                let result =
                    GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
                let _ = CloseHandle(handle);
                result.ok()?;
                // FILETIME is 100-ns ticks since 1601-01-01 UTC.
                // UNIX epoch (1970-01-01) is 11644473600 seconds later.
                let ticks =
                    ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);
                const EPOCH_DIFF_100NS: u64 = 11_644_473_600 * 10_000_000;
                if ticks < EPOCH_DIFF_100NS {
                    return None;
                }
                let unix_100ns = ticks - EPOCH_DIFF_100NS;
                let secs = unix_100ns / 10_000_000;
                let nanos = ((unix_100ns % 10_000_000) * 100) as u32;
                Some(std::time::UNIX_EPOCH + std::time::Duration::new(secs, nanos))
            }
        }
    }

    struct EnumContext {
        results: Vec<WindowEntry>,
        own_pid: u32,
        /// Cache PID → process name across the enumeration cycle. Opening a
        /// process handle per-window is the dominant cost when many windows
        /// share a PID (browser tabs, IDE child windows).
        pid_name_cache: std::collections::HashMap<u32, Option<String>>,
    }

    /// Query DWM cloaked attribute. Returns true if the window is cloaked for
    /// any reason — UWP suspended, on another virtual desktop, or hidden by
    /// the shell. Returns false on any failure (be lenient).
    unsafe fn is_window_cloaked(hwnd: HWND) -> bool {
        use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
        let mut cloaked: u32 = 0;
        let res = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        res.is_ok() && cloaked != 0
    }

    unsafe extern "system" fn enum_window_callback_v2(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam.0 as *mut EnumContext);

        // SAFETY block 1: cheap visibility filters — no allocations, no handles.
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
        if IsIconic(hwnd).as_bool() {
            return TRUE;
        }

        // SAFETY block 2: title read. Buffer length matches GetWindowTextW spec.
        let mut title_buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title_buf);
        if len == 0 {
            return TRUE; // no title — skip
        }
        let title = String::from_utf16_lossy(&title_buf[..len as usize]);

        // SAFETY block 3: class name read.
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, &mut class_buf);
        let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);

        // SAFETY block 4: PID lookup (`pid` is initialized to 0 and overwritten).
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        // PID → name with intra-cycle cache. Cache miss opens the process handle.
        let process_name = match ctx
            .pid_name_cache
            .entry(pid)
            .or_insert_with(|| get_process_name_from_pid(pid))
            .clone()
        {
            Some(name) => name,
            None => return TRUE, // can't identify — skip
        };

        // SAFETY block 5: style + ownership lookups operate on a valid HWND.
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        let is_tool_window = (ex_style & WS_EX_TOOLWINDOW.0) != 0;
        let owner_hwnd = GetWindow(hwnd, GW_OWNER)
            .ok()
            .filter(|o| !o.0.is_null());
        let is_owned = owner_hwnd.is_some();
        // A true application-modal dialog disables its owner (the parent goes
        // grey and can't be clicked). Floating helpers — find/replace, color
        // pickers, tool palettes — leave the owner enabled. We only want to
        // shield the parent process from idle actions when there's a real
        // modal blocking interaction.
        let disables_owner = match owner_hwnd {
            Some(owner) => !IsWindowEnabled(owner).as_bool(),
            None => false,
        };

        // SAFETY block 6: DWM cloaked query. Documented to be safe on any HWND.
        let is_cloaked = is_window_cloaked(hwnd);

        let info = WindowInfo {
            process_name,
            title,
            class_name,
            is_tool_window,
            is_owned,
            disables_owner,
            own_pid: pid == ctx.own_pid,
            is_cloaked,
            // Virtual-desktop hiding manifests as DWM_CLOAKED_SHELL on Win10/11,
            // so the cloaked check already covers off-desktop windows. We keep
            // the field for completeness and default true here.
            is_on_current_desktop: true,
        };

        ctx.results.push(WindowEntry {
            hwnd: hwnd.0 as isize,
            pid,
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
        let result = if QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), pwstr, &mut size)
            .is_ok()
        {
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
        GetWindowThreadProcessId, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
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
    fn process_start_time(&self, _pid: u32) -> Option<std::time::SystemTime> {
        None
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
        pub foreground_hwnd: Arc<Mutex<isize>>,
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

        pub fn set_foreground_hwnd(&self, hwnd: isize) {
            *self.foreground_hwnd.lock().unwrap() = hwnd;
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

        fn get_foreground_hwnd(&self) -> isize {
            *self.foreground_hwnd.lock().unwrap()
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

        fn process_start_time(&self, _pid: u32) -> Option<std::time::SystemTime> {
            None
        }
    }
}
