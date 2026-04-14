# Foreground Event Hook Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace polling-based foreground detection with a Win32 event hook (`SetWinEventHook`) so idle timestamps are precise to the millisecond instead of the 30-second polling interval.

**Architecture:** A shared `Arc<Mutex<HashMap<String, Instant>>>` holds foreground timestamps. On Windows, `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` installs a callback on the main thread (which already runs a Win32 message pump via Slint) that updates this map whenever the foreground window changes. The monitor thread reads from this shared map instead of maintaining its own private `last_foreground` HashMap. The monitor's `poll()` still calls `get_foreground_process()` as a belt-and-suspenders fallback to catch any events the hook might miss during startup.

**Tech Stack:** `windows` crate 0.58 (`Win32_UI_Accessibility` feature for `SetWinEventHook`), existing Slint event loop as the message pump.

---

## File Structure

| File | Change | Responsibility |
|------|--------|---------------|
| `Cargo.toml` | Modify | Add `Win32_UI_Accessibility` feature |
| `src/winapi.rs` | Modify | Add `install_foreground_hook()` function + cleanup guard |
| `src/monitor.rs` | Modify | Replace private `last_foreground` with shared `foreground_timestamps` |
| `src/main.rs` | Modify | Create shared map, install hook, pass map to monitor |

---

### Task 1: Add Win32_UI_Accessibility dependency

**Files:**
- Modify: `Cargo.toml:21-29`

- [ ] **Step 1: Add the feature flag**

In `Cargo.toml`, add `"Win32_UI_Accessibility"` to the windows crate features:

```toml
[target.'cfg(windows)'.dependencies.windows]
version = "0.58"
features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Accessibility",
    "Win32_System_Threading",
    "Win32_System_ProcessStatus",
    "Win32_Graphics_Gdi",
]
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check --target x86_64-pc-windows-gnu`
Expected: compiles with no new errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add Win32_UI_Accessibility for SetWinEventHook"
```

---

### Task 2: Add shared ForegroundTimestamps type alias

**Files:**
- Modify: `src/monitor.rs:1-11`

- [ ] **Step 1: Add the type alias**

At the top of `src/monitor.rs`, after the existing imports, add:

```rust
use std::collections::HashSet;

/// Shared foreground timestamps, updated by the Win32 event hook (real-time)
/// and by the monitor's poll fallback. Read by the monitor for idle calculations.
pub type ForegroundTimestamps = Arc<Mutex<HashMap<String, Instant>>>;
```

Also replace the existing `std::collections::HashSet` usages in `poll()` — they currently use the full path `std::collections::HashSet`. This import makes them cleaner.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles (type alias is unused so far, but no errors)

- [ ] **Step 3: Commit**

```bash
git add src/monitor.rs
git commit -m "refactor: add ForegroundTimestamps type alias"
```

---

### Task 3: Refactor Monitor to use shared foreground timestamps

This is the core change. The monitor's private `last_foreground: HashMap<String, Instant>` becomes a shared `foreground_timestamps: ForegroundTimestamps` passed in at construction.

**Files:**
- Modify: `src/monitor.rs`

- [ ] **Step 1: Update the Monitor struct**

Replace the `last_foreground` field in the `Monitor` struct:

```rust
pub struct Monitor<W: WindowApi> {
    api: W,
    config: Arc<RwLock<Config>>,
    paused: Arc<AtomicBool>,
    /// Shared foreground timestamps — written by the event hook (main thread)
    /// and the poll fallback (this thread). Read during idle checks.
    foreground_timestamps: ForegroundTimestamps,
    current_windows: Vec<WindowEntry>,
    cumulative_idle: HashMap<String, Duration>,
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    previously_managed: HashSet<String>,
}
```

- [ ] **Step 2: Update the constructor**

Change `Monitor::new()` to accept the shared map:

```rust
pub fn new(
    api: W,
    config: Arc<RwLock<Config>>,
    paused: Arc<AtomicBool>,
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    foreground_timestamps: ForegroundTimestamps,
) -> Self {
    Self {
        api,
        config,
        paused,
        foreground_timestamps,
        current_windows: Vec::new(),
        cumulative_idle: HashMap::new(),
        snapshot_buffer,
        previously_managed: HashSet::new(),
    }
}
```

- [ ] **Step 3: Update poll() — newly managed detection**

In `poll()`, the newly-managed grace period code currently does `self.last_foreground.insert(proc.clone(), now)`. Change it to lock the shared map:

```rust
// Inside poll(), the newly-managed block becomes:
let mut newly_managed = HashSet::new();
{
    let mut timestamps = self.foreground_timestamps.lock().unwrap();
    for proc in &currently_managed {
        if !self.previously_managed.contains(proc) {
            timestamps.insert(proc.clone(), now);
            newly_managed.insert(proc.clone());
        }
    }
}
self.previously_managed = currently_managed;
```

- [ ] **Step 4: Update poll() — foreground timestamp update**

The existing code:
```rust
let foreground = self.api.get_foreground_process();
if let Some(ref fg) = foreground {
    let fg_lower = fg.to_lowercase();
    self.last_foreground.insert(fg_lower, now);
}
```

Becomes:
```rust
let foreground = self.api.get_foreground_process();
if let Some(ref fg) = foreground {
    let fg_lower = fg.to_lowercase();
    self.foreground_timestamps.lock().unwrap().insert(fg_lower, now);
}
```

- [ ] **Step 5: Update poll() — idle time lookup**

The idle check currently reads `self.last_foreground.get(&proc_lower)`. It needs to read from the shared map. To avoid holding the lock for the entire loop, take a snapshot at the start of the idle-check section:

```rust
// Right before the `for entry in &self.current_windows` loop:
let fg_snapshot = self.foreground_timestamps.lock().unwrap().clone();
```

Then replace all `self.last_foreground.get(...)` with `fg_snapshot.get(...)` in the loop body.

- [ ] **Step 6: Update poll() — new timestamp insertions**

The deferred `new_timestamps` insertions currently do `self.last_foreground.insert(proc, ts)`. Change to:

```rust
// Apply deferred mutations
{
    let mut timestamps = self.foreground_timestamps.lock().unwrap();
    for (proc, ts) in new_timestamps {
        timestamps.insert(proc, ts);
    }
}
```

- [ ] **Step 7: Update get_active_windows_snapshot()**

This method reads `self.last_foreground`. Change it to lock the shared map:

```rust
pub fn get_active_windows_snapshot(&self) -> Vec<ActiveWindowSnapshot> {
    let config = match self.config.read() {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let now = Instant::now();
    let fg_snapshot = self.foreground_timestamps.lock().unwrap().clone();
    let mut snapshots: Vec<ActiveWindowSnapshot> = self
        .current_windows
        .iter()
        .filter(|e| !filter::is_system_window(&e.info))
        .filter(|e| !config.is_hidden(&e.info.process_name))
        .map(|e| {
            let proc_lower = e.info.process_name.to_lowercase();
            let idle_secs = fg_snapshot
                .get(&proc_lower)
                .map(|t| now.duration_since(*t).as_secs())
                .unwrap_or(0);
            let managed = config.resolve_process(&proc_lower).is_some();

            ActiveWindowSnapshot {
                process: e.info.process_name.clone(),
                title: e.info.title.clone(),
                idle_secs,
                managed,
            }
        })
        .collect();

    snapshots.sort_by(|a, b| b.idle_secs.cmp(&a.idle_secs));
    snapshots
}
```

- [ ] **Step 8: Update track_cumulative_idle()**

```rust
fn track_cumulative_idle(&mut self, process: &str, now: Instant) {
    let timestamps = self.foreground_timestamps.lock().unwrap();
    if let Some(last_fg) = timestamps.get(process) {
        let idle = now.duration_since(*last_fg);
        let entry = self.cumulative_idle.entry(process.to_string()).or_default();
        *entry += idle;
    }
}
```

- [ ] **Step 9: Update test helper setup()**

The test `setup()` function needs to create and pass the shared map:

```rust
fn setup(
    config: Config,
) -> (Monitor<MockWindowApi>, MockWindowApi, Arc<RwLock<Config>>, Arc<AtomicBool>, ForegroundTimestamps) {
    let mock = MockWindowApi::new();
    let config = Arc::new(RwLock::new(config));
    let paused = Arc::new(AtomicBool::new(false));
    let snapshot_buffer = Arc::new(Mutex::new(Vec::new()));
    let foreground_timestamps: ForegroundTimestamps = Arc::new(Mutex::new(HashMap::new()));
    let monitor = Monitor::new(
        mock.clone(),
        config.clone(),
        paused.clone(),
        snapshot_buffer,
        foreground_timestamps.clone(),
    );
    let mock_ref = mock.clone();
    (monitor, mock_ref, config, paused, foreground_timestamps)
}
```

- [ ] **Step 10: Update all tests**

Every test that destructures `setup()` needs to add the 5th element. Every test that writes `monitor.last_foreground.insert(...)` needs to write to the shared map instead.

Pattern — old:
```rust
let (mut monitor, mock, _, _) = setup(config);
monitor.last_foreground.insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(1));
```

New:
```rust
let (mut monitor, mock, _, _, timestamps) = setup(config);
timestamps.lock().unwrap().insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(1));
```

Tests that read `monitor.last_foreground.contains_key(...)` become:
```rust
assert!(timestamps.lock().unwrap().contains_key("notepad.exe"));
```

Apply this to every test in the file:
- `test_foreground_process_not_acted_on` — destructure only
- `test_idle_process_gets_minimized` — destructure + insert
- `test_idle_process_gets_closed` — destructure + insert
- `test_unmanaged_process_never_acted_on` — destructure + insert
- `test_paused_skips_poll` — destructure + insert
- `test_process_returns_to_foreground_resets_timer` — destructure + insert + read
- `test_disabled_rule_not_acted_on` — destructure + insert
- `test_bucket_triggers_action` — destructure + insert
- `test_system_window_skipped` — destructure + insert
- `test_config_change_takes_effect` — destructure + insert
- `test_first_sighting_gets_grace_period` — destructure + read
- `test_active_windows_snapshot_sorted` — destructure + insert (uses own setup)
- `test_snapshot_buffer_populated_after_poll` — uses own setup (needs foreground_timestamps param)
- `test_snapshot_buffer_empty_when_paused` — uses own setup (needs foreground_timestamps param)
- `test_newly_managed_process_gets_grace_period` — destructure + insert

- [ ] **Step 11: Run tests**

Run: `cargo test`
Expected: all 40 tests pass

- [ ] **Step 12: Commit**

```bash
git add src/monitor.rs
git commit -m "refactor: monitor uses shared ForegroundTimestamps map"
```

---

### Task 4: Add install_foreground_hook() to winapi.rs

**Files:**
- Modify: `src/winapi.rs`

- [ ] **Step 1: Add the hook function (Windows implementation)**

After the closing `}` of `mod win32_impl` (after line 245), add:

```rust
/// Handle guard for the Win32 foreground event hook. Unhooks on drop.
#[cfg(target_os = "windows")]
pub struct ForegroundHookGuard {
    handle: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
}

#[cfg(target_os = "windows")]
impl Drop for ForegroundHookGuard {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::UI::Accessibility::UnhookWinEvent(self.handle);
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
```

- [ ] **Step 2: Make get_process_name_from_pid accessible to the hook callback**

The function `get_process_name_from_pid` is currently defined inside `mod win32_impl` as a private function. Change its visibility to `pub(super)` so the hook callback (which lives in the parent module scope but calls into `win32_impl`) can access it:

In `src/winapi.rs` inside `mod win32_impl`, change:
```rust
unsafe fn get_process_name_from_pid(pid: u32) -> Option<String> {
```
to:
```rust
pub(super) unsafe fn get_process_name_from_pid(pid: u32) -> Option<String> {
```

- [ ] **Step 3: Add non-Windows stubs**

After the existing non-Windows `impl WindowApi for Win32Api` block, add:

```rust
#[cfg(not(target_os = "windows"))]
pub struct ForegroundHookGuard;

#[cfg(not(target_os = "windows"))]
pub fn install_foreground_hook(
    _timestamps: crate::monitor::ForegroundTimestamps,
) -> Result<ForegroundHookGuard, String> {
    Ok(ForegroundHookGuard)
}
```

- [ ] **Step 4: Verify it compiles for both targets**

Run: `cargo check` (Linux)
Run: `cargo check --target x86_64-pc-windows-gnu` (Windows cross)
Expected: both compile

- [ ] **Step 5: Commit**

```bash
git add src/winapi.rs
git commit -m "feat: add SetWinEventHook-based foreground tracking"
```

---

### Task 5: Wire the hook into main.rs

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Create the shared timestamp map and install the hook**

In `main()`, after the `snapshot_buffer` creation (around line 30) and before the Slint window creation, add:

```rust
let foreground_timestamps: monitor::ForegroundTimestamps =
    Arc::new(Mutex::new(std::collections::HashMap::new()));
```

Then after the Slint event loop setup but before the monitor thread spawn (before the "Spawn monitor thread" comment), install the hook:

```rust
// Install foreground event hook (updates timestamps in real-time via Win32 callback)
let _foreground_hook = match winapi::install_foreground_hook(foreground_timestamps.clone()) {
    Ok(guard) => Some(guard),
    Err(e) => {
        log::warn!("Failed to install foreground hook, falling back to polling: {}", e);
        None
    }
};
```

The `_foreground_hook` guard must live until the event loop ends so the hook stays active. It's stored as a local in `main()` and dropped during cleanup.

- [ ] **Step 2: Pass the shared map to Monitor::new()**

Update the monitor thread spawn to pass the timestamps:

```rust
let monitor_timestamps = foreground_timestamps.clone();
let monitor_thread = std::thread::spawn(move || {
    let api = Win32Api::new();
    let mut monitor = Monitor::new(
        api,
        monitor_config,
        monitor_paused,
        monitor_snapshot,
        monitor_timestamps,
    );
    monitor.run(monitor_stop);
});
```

- [ ] **Step 3: Also update run_headless()**

The `run_headless` fallback function also creates a Monitor. Update it:

```rust
fn run_headless(config: Arc<RwLock<Config>>, paused: Arc<AtomicBool>, should_stop: Arc<AtomicBool>) {
    log::warn!("Running in headless mode (no GUI)");
    let api = Win32Api::new();
    let dummy_buffer = Arc::new(Mutex::new(Vec::new()));
    let foreground_timestamps: monitor::ForegroundTimestamps =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut monitor = Monitor::new(api, config, paused, dummy_buffer, foreground_timestamps);
    monitor.run(should_stop);
}
```

- [ ] **Step 4: Verify full build**

Run: `cargo test`
Expected: all 40 tests pass

Run: `cargo build --release --target x86_64-pc-windows-gnu`
Expected: clean build, no warnings

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire foreground event hook into main startup"
```

---

### Task 6: Add a test for hook-injected timestamps

**Files:**
- Modify: `src/monitor.rs` (test section)

- [ ] **Step 1: Write a test that simulates hook behavior**

The hook writes to the shared `ForegroundTimestamps` map from a different context (simulating the main thread). This test verifies the monitor correctly reads hook-injected timestamps for idle calculations:

```rust
#[test]
fn test_external_timestamp_update_respected() {
    // Simulates the foreground hook updating timestamps externally
    let config = make_config(
        vec![AppRule {
            process: "notepad.exe".into(),
            timeout_mins: 1,
            action: Action::Minimize,
            enabled: true,
        }],
        vec![],
    );
    let (mut monitor, mock, _, _, timestamps) = setup(config);

    mock.set_foreground(Some("other.exe"));
    mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);

    // Simulate hook setting notepad as recently active (5 seconds ago)
    timestamps.lock().unwrap().insert(
        "notepad.exe".to_string(),
        Instant::now() - Duration::from_secs(5),
    );

    monitor.poll();

    // 5 seconds < 1 minute timeout — should NOT minimize
    assert!(mock.get_minimized().is_empty());

    // Now simulate hook hasn't fired for a long time (notepad idle 2 minutes)
    timestamps.lock().unwrap().insert(
        "notepad.exe".to_string(),
        Instant::now() - Duration::from_secs(120),
    );

    monitor.poll();

    // 120 seconds > 60 second timeout — should minimize
    assert!(!mock.get_minimized().is_empty());
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test test_external_timestamp_update_respected -- --nocapture`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/monitor.rs
git commit -m "test: verify monitor respects externally-injected foreground timestamps"
```

---

## Verification Checklist

After all tasks are complete:

- [ ] `cargo test` — all tests pass (41 total)
- [ ] `cargo build --release --target x86_64-pc-windows-gnu` — clean build, no warnings
- [ ] `cargo check` — clean on Linux
- [ ] Manual test on Windows: open Fade, switch between windows, verify the Active Windows tab shows accurate idle times that update in near-real-time (not lagging by 30 seconds)
