/// Core idle-detection polling loop.
/// Tracks which processes were last in the foreground and triggers
/// minimize/close actions when idle timeouts are exceeded.
use crate::config::{Action, Config, RuleSource};
use crate::filter;
use crate::winapi::{WindowApi, WindowEntry};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Shared foreground timestamps, updated by the Win32 event hook (real-time)
/// and by the monitor's poll fallback. Read by the monitor for idle calculations.
pub type ForegroundTimestamps = Arc<Mutex<HashMap<String, Instant>>>;

/// Snapshot of a tracked window for the GUI's active-windows view.
#[derive(Debug, Clone)]
pub struct ActiveWindowSnapshot {
    pub process: String,
    /// Kept for potential future UI use (tooltips, per-window detail).
    /// The current unified UI dedupes by process and does not display titles.
    #[allow(dead_code)]
    pub title: String,
    pub idle_secs: u64,
    /// Seconds since Fade first observed this process running.
    /// Kept for potential future display; current UI shows idle/last-active instead.
    #[allow(dead_code)]
    pub open_secs: u64,
}

/// One entry in the activity log — an action Fade took.
#[derive(Debug, Clone)]
pub struct ActionLogEntry {
    pub process: String,
    pub title: String,
    pub action: Action,
    /// Seconds since UNIX epoch (for display formatting on the GUI side).
    pub timestamp: u64,
    /// Whether the rule that triggered this action came from an app_rule or a
    /// bucket. Persisted for debugging; the GUI does not currently surface it.
    #[allow(dead_code)]
    pub rule_source: RuleSource,
    /// Timeout (minutes) that was in effect when the action fired.
    #[allow(dead_code)]
    pub timeout_mins: u64,
}

/// Shared ring-buffer of recent actions. Most-recent first.
pub type ActionLog = Arc<Mutex<std::collections::VecDeque<ActionLogEntry>>>;

const ACTION_LOG_CAPACITY: usize = 500;

/// The monitor that runs the polling loop.
pub struct Monitor<W: WindowApi> {
    api: W,
    config: Arc<RwLock<Config>>,
    paused: Arc<AtomicBool>,
    /// Last time each process (lowercase) was the foreground window.
    foreground_timestamps: ForegroundTimestamps,
    /// HWND lookup: process_name_lower -> vec of (hwnd, title)
    /// Updated each poll cycle from enumeration.
    current_windows: Vec<WindowEntry>,
    /// Shared buffer for GUI to read active window snapshots.
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    /// Processes that had active rules last poll (lowercase).
    /// Used to detect newly managed processes and reset their idle clock.
    previously_managed: HashSet<String>,
    /// First time a (process_name_lower, pid) pair was observed. Cleared when the
    /// PID stops appearing in polls. Used as a fallback when process_start_time
    /// isn't available (access denied, protected process, etc).
    first_seen: HashMap<(String, u32), Instant>,
    /// Cached process creation time per PID from Win32 GetProcessTimes.
    /// None = query was attempted and failed (don't retry every poll).
    process_start_cache: HashMap<u32, Option<std::time::SystemTime>>,
    /// Ring-buffer of recent actions for the Activity GUI tab.
    action_log: ActionLog,
    /// Instant of the previous successful poll. Used to detect resume-from-sleep
    /// and other monotonic-clock gaps so we don't immediately fire actions
    /// across a system suspend/lock interval.
    last_poll_at: Option<Instant>,
}

/// If the gap between two polls exceeds the expected interval by more than
/// this factor (e.g. >5× the poll interval), assume the system suspended,
/// locked, or hibernated and rebase idle timestamps to "now". This prevents
/// surprise closes the instant the user unlocks.
const POLL_GAP_FACTOR: u32 = 5;
const POLL_GAP_FLOOR_SECS: u64 = 60;

impl<W: WindowApi> Monitor<W> {
    pub fn new(
        api: W,
        config: Arc<RwLock<Config>>,
        paused: Arc<AtomicBool>,
        snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
        foreground_timestamps: ForegroundTimestamps,
        action_log: ActionLog,
    ) -> Self {
        Self {
            api,
            config,
            paused,
            foreground_timestamps,
            current_windows: Vec::new(),
            snapshot_buffer,
            previously_managed: HashSet::new(),
            first_seen: HashMap::new(),
            process_start_cache: HashMap::new(),
            action_log,
            last_poll_at: None,
        }
    }

    fn record_action(
        &self,
        process: &str,
        title: &str,
        action: Action,
        rule_source: RuleSource,
        timeout_mins: u64,
    ) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut log) = self.action_log.lock() {
            log.push_front(ActionLogEntry {
                process: process.to_string(),
                title: title.to_string(),
                action,
                timestamp: ts,
                rule_source,
                timeout_mins,
            });
            while log.len() > ACTION_LOG_CAPACITY {
                log.pop_back();
            }
        }
    }

    /// Run one poll cycle. Called every `polling_interval_secs`.
    pub fn poll(&mut self) {
        if self.paused.load(Ordering::Relaxed) {
            return;
        }

        let config = match self.config.read() {
            Ok(c) => c.clone(),
            Err(_) => {
                log::error!("Config lock poisoned, skipping poll");
                return;
            }
        };

        let now = Instant::now();

        // 0a. Detect resume-from-sleep / lock / hibernation: if the gap since
        // the previous poll dwarfs the expected interval, treat every tracked
        // process as freshly active so we don't immediately act on something
        // the user is about to come back to.
        let interval_secs = config.general.polling_interval_secs.max(1);
        let threshold =
            Duration::from_secs((interval_secs * POLL_GAP_FACTOR as u64).max(POLL_GAP_FLOOR_SECS));
        let resumed_from_gap = match self.last_poll_at {
            Some(prev) => now.duration_since(prev) > threshold,
            None => false,
        };
        if resumed_from_gap {
            log::warn!(
                "Poll gap exceeded {:?}; treating as suspend/resume — rebasing idle timestamps",
                threshold
            );
            if let Ok(mut ts) = self.foreground_timestamps.lock() {
                for v in ts.values_mut() {
                    *v = now;
                }
            }
        }
        self.last_poll_at = Some(now);

        // 0. Detect newly managed processes and reset their idle clock.
        // This prevents immediate action when a rule is added for an already-open window.
        // Must match resolve_process semantics: a disabled AppRule explicitly
        // excludes its process, even if an enabled bucket would otherwise
        // include it. Otherwise re-enabling that AppRule later wouldn't be
        // detected as "newly managed" (the process was in previously_managed
        // via the bucket) and an already-idle window could be acted on
        // immediately without the grace period.
        let mut candidates: HashSet<String> = HashSet::new();
        for rule in &config.app_rule {
            candidates.insert(rule.process.to_lowercase());
        }
        for bucket in &config.bucket {
            for proc in &bucket.processes {
                candidates.insert(proc.to_lowercase());
            }
        }
        let currently_managed: HashSet<String> = candidates
            .into_iter()
            .filter(|p| config.resolve_process(p).is_some())
            .collect();
        let mut newly_managed = HashSet::new();
        if let Ok(mut timestamps) = self.foreground_timestamps.lock() {
            for proc in &currently_managed {
                if !self.previously_managed.contains(proc) {
                    timestamps.insert(proc.clone(), now);
                    newly_managed.insert(proc.clone());
                }
            }
        }
        self.previously_managed = currently_managed;

        // 1. Update foreground process timestamp
        let foreground = self.api.get_foreground_process();
        if let Some(ref fg) = foreground {
            let fg_lower = fg.to_lowercase();
            if let Ok(mut ts) = self.foreground_timestamps.lock() {
                ts.insert(fg_lower, now);
            }
        }

        // 2. Enumerate visible windows and store for GUI snapshot
        self.current_windows = self.api.enumerate_visible_windows();

        // 2a. Track first-seen per (process, pid) and cache GetProcessTimes result.
        // first_seen is the fallback when process_start_time is unavailable.
        let current_keys: HashSet<(String, u32)> = self
            .current_windows
            .iter()
            .filter(|e| !filter::is_system_window(&e.info))
            .map(|e| (e.info.process_name.to_lowercase(), e.pid))
            .collect();
        for key in &current_keys {
            self.first_seen.entry(key.clone()).or_insert(now);
        }
        self.first_seen.retain(|k, _| current_keys.contains(k));

        let current_pids: HashSet<u32> = current_keys.iter().map(|(_, p)| *p).collect();
        for pid in &current_pids {
            self.process_start_cache
                .entry(*pid)
                .or_insert_with(|| self.api.process_start_time(*pid));
        }
        self.process_start_cache
            .retain(|pid, _| current_pids.contains(pid));

        // 2b. Prune foreground_timestamps to bound memory growth.
        // Retain entries only for processes that are currently visible or the current
        // foreground. A process that disappears and reappears loses its prior timestamp
        // (re-seeded on next foreground switch), which is acceptable — idle tracking
        // restarts from the new sighting.
        let current_procs: HashSet<String> = current_keys.iter().map(|(p, _)| p.clone()).collect();
        let fg_keep = foreground.as_deref().map(|s| s.to_lowercase());
        if let Ok(mut ts) = self.foreground_timestamps.lock() {
            ts.retain(|proc, _| {
                current_procs.contains(proc) || fg_keep.as_deref() == Some(proc.as_str())
            });
        }

        // 3-7. Check each window against rules and act
        // Collect actions first to avoid borrow conflicts.
        let fg_lower = foreground.as_deref().map(|s| s.to_lowercase());

        struct PendingAction {
            hwnd: isize,
            process: String,
            title: String,
            action: Action,
            idle_secs: f64,
            rule_source: RuleSource,
            timeout_mins: u64,
        }

        let mut actions: Vec<PendingAction> = Vec::new();
        let mut new_timestamps: Vec<(String, Instant)> = Vec::new();

        let fg_snapshot = match self.foreground_timestamps.lock() {
            Ok(ts) => ts.clone(),
            Err(e) => {
                log::error!("Foreground timestamps lock poisoned, skipping poll: {}", e);
                return;
            }
        };

        // PIDs that currently have a true application-modal dialog open — i.e.
        // an owned window whose owner has been disabled by Windows. Acting on
        // the parent of an active modal can interrupt save/auth dialogs, so we
        // skip the entire process. Owned-but-not-disabling helpers (find /
        // replace, color picker, tool palettes) don't qualify. We must NOT
        // pre-filter with is_system_window here: legitimate dialogs frequently
        // look "system-like" but are exactly what this guard exists to protect.
        // Include both the dialog's PID and the owner's PID. For most modals
        // these are the same, but out-of-process modals (shell-hosted picker
        // dialogs over an app) have distinct PIDs — without the owner's PID
        // the true parent window would still be eligible for idle actions.
        let mut pids_with_modal: HashSet<u32> = HashSet::new();
        for entry in &self.current_windows {
            if entry.info.disables_owner {
                pids_with_modal.insert(entry.pid);
                if let Some(opid) = entry.info.owner_pid {
                    pids_with_modal.insert(opid);
                }
            }
        }

        for entry in &self.current_windows {
            let proc_lower = entry.info.process_name.to_lowercase();

            // Skip the currently foreground process
            if fg_lower.as_deref() == Some(proc_lower.as_str()) {
                continue;
            }

            // Skip system/filtered windows
            if filter::is_system_window(&entry.info) {
                continue;
            }

            // Skip owned windows (modals, popups, dialogs) — they belong to a parent.
            if entry.info.is_owned {
                continue;
            }

            // Skip windows whose process currently has a real modal dialog
            // visible (an owned window that disables its parent). Closing the
            // parent would tear the modal down mid-interaction.
            if pids_with_modal.contains(&entry.pid) {
                log::debug!(
                    "Skipping {} ({}) — process has an active modal dialog",
                    entry.info.process_name,
                    entry.info.title
                );
                continue;
            }

            // Skip processes that just became managed this poll cycle
            if newly_managed.contains(&proc_lower) {
                continue;
            }

            // Look up rule
            let rule = match config.resolve_process(&proc_lower) {
                Some(r) => r,
                None => continue,
            };

            // Check idle time
            let last_fg = fg_snapshot.get(&proc_lower).copied();
            let idle_duration = match last_fg {
                Some(t) => now.duration_since(t),
                None => {
                    // First time seeing this process — start tracking now
                    new_timestamps.push((proc_lower, now));
                    continue;
                }
            };

            let timeout = Duration::from_secs(rule.timeout_mins * 60);
            if idle_duration < timeout {
                continue;
            }

            // Skip fullscreen windows
            if self.api.is_fullscreen(entry.hwnd) {
                log::debug!(
                    "Skipping fullscreen window: {} ({})",
                    entry.info.process_name,
                    entry.info.title
                );
                continue;
            }

            // Double-check window is still valid
            if !self.api.is_window_valid(entry.hwnd) {
                log::debug!(
                    "Window vanished: {} ({})",
                    entry.info.process_name,
                    entry.info.title
                );
                continue;
            }

            // Double-check it's not now the foreground (HWND-level — the
            // process-name check above is conservative for same-process
            // windows but cannot distinguish two HWNDs sharing a process name).
            let current_fg_hwnd = self.api.get_foreground_hwnd();
            if current_fg_hwnd != 0 && current_fg_hwnd == entry.hwnd {
                new_timestamps.push((proc_lower, Instant::now()));
                continue;
            }
            if let Some(current_fg) = self.api.get_foreground_process() {
                if current_fg.to_lowercase() == proc_lower {
                    new_timestamps.push((proc_lower, Instant::now()));
                    continue;
                }
            }

            actions.push(PendingAction {
                hwnd: entry.hwnd,
                process: entry.info.process_name.clone(),
                title: entry.info.title.clone(),
                action: rule.action.clone(),
                idle_secs: idle_duration.as_secs_f64(),
                rule_source: rule.source,
                timeout_mins: rule.timeout_mins,
            });
        }

        // Apply deferred timestamp mutations
        if !new_timestamps.is_empty() {
            if let Ok(mut timestamps) = self.foreground_timestamps.lock() {
                for (proc, ts) in new_timestamps {
                    timestamps.insert(proc, ts);
                }
            }
        }

        // Execute actions. Window titles can contain sensitive content
        // (document names, URLs, draft subject lines) so the info-level log
        // line omits them; the full title is preserved at debug level and in
        // the in-memory action log used by the GUI.
        for action in actions {
            let verb = match action.action {
                Action::Minimize => "Minimizing",
                Action::Close => "Closing",
            };
            log::info!(
                "{verb}: {} — idle {:.0}s [source={}, timeout={}m]",
                action.process,
                action.idle_secs,
                action.rule_source.as_str(),
                action.timeout_mins,
            );
            log::debug!("{verb} hwnd={:#x} title={:?}", action.hwnd, action.title);
            match action.action {
                Action::Minimize => self.api.minimize_window(action.hwnd),
                Action::Close => self.api.close_window(action.hwnd),
            }
            self.record_action(
                &action.process,
                &action.title,
                action.action,
                action.rule_source,
                action.timeout_mins,
            );
        }

        // Publish snapshot for GUI
        self.publish_snapshot();
    }

    fn publish_snapshot(&self) {
        let snapshot = self.get_active_windows_snapshot();
        if let Ok(mut buf) = self.snapshot_buffer.lock() {
            *buf = snapshot;
        }
    }

    /// Get a snapshot of active windows for the GUI.
    pub fn get_active_windows_snapshot(&self) -> Vec<ActiveWindowSnapshot> {
        let now = Instant::now();
        let fg_snapshot = match self.foreground_timestamps.lock() {
            Ok(ts) => ts.clone(),
            Err(_) => return Vec::new(),
        };
        let mut snapshots: Vec<ActiveWindowSnapshot> = self
            .current_windows
            .iter()
            .filter(|e| !filter::is_system_window(&e.info))
            .map(|e| {
                let proc_lower = e.info.process_name.to_lowercase();
                let idle_secs = fg_snapshot
                    .get(&proc_lower)
                    .map(|t| now.duration_since(*t).as_secs())
                    .unwrap_or(0);

                let open_secs = match self
                    .process_start_cache
                    .get(&e.pid)
                    .and_then(|o| o.as_ref())
                {
                    Some(start) => std::time::SystemTime::now()
                        .duration_since(*start)
                        .map(|d| d.as_secs())
                        .unwrap_or(0),
                    None => self
                        .first_seen
                        .get(&(proc_lower.clone(), e.pid))
                        .map(|t| now.duration_since(*t).as_secs())
                        .unwrap_or(0),
                };
                ActiveWindowSnapshot {
                    process: e.info.process_name.clone(),
                    title: e.info.title.clone(),
                    idle_secs,
                    open_secs,
                }
            })
            .collect();

        // Sort by idle time descending (longest idle first)
        snapshots.sort_by_key(|s| std::cmp::Reverse(s.idle_secs));

        snapshots
    }

    /// Run the monitor loop. Blocks until `should_stop` is set.
    pub fn run(&mut self, should_stop: Arc<AtomicBool>) {
        loop {
            if should_stop.load(Ordering::Relaxed) {
                break;
            }

            self.poll();

            let interval = self
                .config
                .read()
                .map(|c| c.general.polling_interval_secs)
                .unwrap_or(30)
                .max(1);

            // Sleep in short increments so we can check should_stop
            let total = Duration::from_secs(interval);
            let step = Duration::from_millis(500);
            let mut elapsed = Duration::ZERO;
            while elapsed < total {
                if should_stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(step.min(total - elapsed));
                elapsed += step;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Action, AppRule, Bucket, Config, General};
    use crate::filter::WindowInfo;
    use crate::winapi::mock::MockWindowApi;
    use crate::winapi::WindowEntry;

    fn make_config(rules: Vec<AppRule>, buckets: Vec<Bucket>) -> Config {
        Config {
            general: General::default(),
            app_rule: rules,
            bucket: buckets,
        }
    }

    fn make_entry(hwnd: isize, process: &str, title: &str) -> WindowEntry {
        WindowEntry {
            hwnd,
            pid: hwnd as u32,
            info: WindowInfo {
                process_name: process.into(),
                title: title.into(),
                class_name: "AppWindow".into(),
                is_tool_window: false,
                is_owned: false,
                disables_owner: false,
                owner_pid: None,
                own_pid: false,
                is_cloaked: false,
                is_on_current_desktop: true,
            },
        }
    }

    fn setup(
        config: Config,
    ) -> (
        Monitor<MockWindowApi>,
        MockWindowApi,
        Arc<RwLock<Config>>,
        Arc<AtomicBool>,
        ForegroundTimestamps,
    ) {
        let mock = MockWindowApi::new();
        let config = Arc::new(RwLock::new(config));
        let paused = Arc::new(AtomicBool::new(false));
        let snapshot_buffer = Arc::new(Mutex::new(Vec::new()));
        let foreground_timestamps: ForegroundTimestamps = Arc::new(Mutex::new(HashMap::new()));
        let action_log: ActionLog = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let monitor = Monitor::new(
            mock.clone(),
            config.clone(),
            paused.clone(),
            snapshot_buffer,
            foreground_timestamps.clone(),
            action_log,
        );
        let mock_ref = mock.clone();
        (monitor, mock_ref, config, paused, foreground_timestamps)
    }

    #[test]
    fn test_foreground_process_not_acted_on() {
        let config = make_config(
            vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 0, // immediate timeout
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, _timestamps) = setup(config);

        mock.set_foreground(Some("chrome.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);

        // Even with timeout=0, foreground process should not be touched
        // First poll: sets the foreground timestamp
        monitor.poll();
        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_idle_process_gets_minimized() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0, // immediate
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        // First poll: notepad is foreground, sets timestamp
        mock.set_foreground(Some("notepad.exe"));
        mock.set_windows(vec![make_entry(100, "notepad.exe", "Untitled")]);
        monitor.poll();

        // Second poll: something else is foreground, notepad has been idle
        // With timeout_mins=0, it should be acted on
        mock.set_foreground(Some("other.exe"));
        // Need to backdate the timestamp
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(1),
        );
        monitor.poll();

        // The monitor uses enumerate_with_hwnds which creates synthetic HWNDs (0-indexed)
        // so the actual hwnd used will be 0 not 100
        assert!(!mock.get_minimized().is_empty());
    }

    #[test]
    fn test_idle_process_gets_closed() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Close,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        mock.set_foreground(Some("notepad.exe"));
        mock.set_windows(vec![make_entry(100, "notepad.exe", "Untitled")]);
        monitor.poll();

        mock.set_foreground(Some("other.exe"));
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(1),
        );
        monitor.poll();

        assert!(!mock.get_closed().is_empty());
    }

    #[test]
    fn test_unmanaged_process_never_acted_on() {
        let config = make_config(vec![], vec![]);
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "unmanaged.exe", "Something")]);
        timestamps.lock().unwrap().insert(
            "unmanaged.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_minimized().is_empty());
        assert!(mock.get_closed().is_empty());
    }

    #[test]
    fn test_paused_skips_poll() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, paused, timestamps) = setup(config);

        paused.store(true, Ordering::Relaxed);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_process_returns_to_foreground_resets_timer() {
        let config = make_config(
            vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 1,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        // Chrome goes idle
        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        timestamps.lock().unwrap().insert(
            "chrome.exe".to_string(),
            Instant::now() - Duration::from_secs(30),
        );
        monitor.poll();
        assert!(mock.get_minimized().is_empty()); // 30s < 60s timeout

        // Chrome comes back to foreground — timer resets
        mock.set_foreground(Some("chrome.exe"));
        monitor.poll();

        // Now check the timestamp was reset to ~now
        let ts = *timestamps.lock().unwrap().get("chrome.exe").unwrap();
        assert!(ts.elapsed().as_secs() < 2);
    }

    #[test]
    fn test_disabled_rule_not_acted_on() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: false, // disabled,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_bucket_triggers_action() {
        let config = make_config(
            vec![],
            vec![Bucket {
                name: "Browsing".into(),
                processes: vec!["chrome.exe".into()],
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                expanded: true,
                icon: None,
            }],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        mock.set_foreground(Some("chrome.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        monitor.poll();

        mock.set_foreground(Some("other.exe"));
        timestamps.lock().unwrap().insert(
            "chrome.exe".to_string(),
            Instant::now() - Duration::from_secs(1),
        );
        monitor.poll();

        assert!(!mock.get_minimized().is_empty());
    }

    #[test]
    fn test_system_window_skipped() {
        let config = make_config(
            vec![AppRule {
                process: "dwm.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![WindowEntry {
            hwnd: 1,
            pid: 1,
            info: WindowInfo {
                process_name: "dwm.exe".into(),
                title: "Desktop Window Manager".into(),
                class_name: "DWM".into(),
                is_tool_window: false,
                is_owned: false,
                disables_owner: false,
                owner_pid: None,
                own_pid: false,
                is_cloaked: false,
                is_on_current_desktop: true,
            },
        }]);
        timestamps.lock().unwrap().insert(
            "dwm.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_config_change_takes_effect() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 999, // very long timeout
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, config_arc, _, timestamps) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(60),
        );
        monitor.poll();
        assert!(mock.get_minimized().is_empty()); // 60s < 999min

        // Change config to immediate timeout
        {
            let mut cfg = config_arc.write().unwrap();
            cfg.app_rule[0].timeout_mins = 0;
        }
        monitor.poll();
        assert!(!mock.get_minimized().is_empty());
    }

    #[test]
    fn test_first_sighting_gets_grace_period() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        // First time seeing notepad — should set timestamp, not act
        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        // Do NOT pre-set foreground_timestamps — this is first sighting
        monitor.poll();

        assert!(mock.get_minimized().is_empty());
        // Now the timestamp should be set
        assert!(timestamps.lock().unwrap().contains_key("notepad.exe"));
    }

    #[test]
    fn test_active_windows_snapshot_sorted() {
        let config = make_config(
            vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![
            make_entry(1, "chrome.exe", "Google"),
            make_entry(2, "notepad.exe", "Untitled"),
        ]);

        // Set different idle times
        {
            let mut ts = timestamps.lock().unwrap();
            ts.insert(
                "chrome.exe".to_string(),
                Instant::now() - Duration::from_secs(300),
            );
            ts.insert(
                "notepad.exe".to_string(),
                Instant::now() - Duration::from_secs(60),
            );
        }

        monitor.poll();

        let snapshot = monitor.get_active_windows_snapshot();
        assert!(!snapshot.is_empty());
        // First should be the longest idle
        if snapshot.len() >= 2 {
            assert!(snapshot[0].idle_secs >= snapshot[1].idle_secs);
        }
    }

    #[test]
    fn test_snapshot_buffer_populated_after_poll() {
        let config = make_config(vec![], vec![]);
        let mock = MockWindowApi::new();
        let config_arc = Arc::new(RwLock::new(config));
        let paused = Arc::new(AtomicBool::new(false));
        let snapshot_buffer = Arc::new(Mutex::new(Vec::new()));
        let foreground_timestamps: ForegroundTimestamps = Arc::new(Mutex::new(HashMap::new()));
        let action_log: ActionLog = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let mut monitor = Monitor::new(
            mock.clone(),
            config_arc,
            paused,
            snapshot_buffer.clone(),
            foreground_timestamps.clone(),
            action_log,
        );

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        foreground_timestamps.lock().unwrap().insert(
            "chrome.exe".to_string(),
            Instant::now() - Duration::from_secs(10),
        );
        monitor.poll();

        let buf = snapshot_buffer.lock().unwrap();
        assert!(!buf.is_empty());
        assert_eq!(buf[0].process, "chrome.exe");
    }

    #[test]
    fn test_snapshot_buffer_empty_when_paused() {
        let config = make_config(vec![], vec![]);
        let mock = MockWindowApi::new();
        let config_arc = Arc::new(RwLock::new(config));
        let paused = Arc::new(AtomicBool::new(true));
        let snapshot_buffer = Arc::new(Mutex::new(Vec::new()));
        let foreground_timestamps: ForegroundTimestamps = Arc::new(Mutex::new(HashMap::new()));
        let action_log: ActionLog = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let mut monitor = Monitor::new(
            mock.clone(),
            config_arc,
            paused,
            snapshot_buffer.clone(),
            foreground_timestamps,
            action_log,
        );

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        monitor.poll();

        let buf = snapshot_buffer.lock().unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_newly_managed_process_gets_grace_period() {
        // Start with no rules — notepad is unmanaged
        let config = make_config(vec![], vec![]);
        let (mut monitor, mock, config_arc, _, timestamps) = setup(config);

        // notepad is open and was last in foreground a long time ago
        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();
        assert!(mock.get_minimized().is_empty()); // unmanaged, no action

        // Now add a rule with timeout=0 (immediate)
        {
            let mut cfg = config_arc.write().unwrap();
            cfg.app_rule.push(AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            });
        }

        // First poll after rule added: should NOT minimize (grace period)
        monitor.poll();
        assert!(mock.get_minimized().is_empty());

        // Second poll: now it should act (idle clock was reset, but timeout=0)
        monitor.poll();
        assert!(!mock.get_minimized().is_empty());
    }

    #[test]
    fn test_owned_window_not_acted_on() {
        // An owned (modal/popup) window should never be a direct action target.
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Close,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        let mut owned = make_entry(1, "notepad.exe", "Save As");
        owned.info.is_owned = true;

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![owned]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_closed().is_empty());
        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_parent_skipped_when_owned_modal_exists() {
        // If a process has an owned/modal window open, the parent window should
        // also be skipped — closing it would tear down the active modal.
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Close,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        let parent = make_entry(10, "notepad.exe", "Untitled - Notepad");
        let mut modal = make_entry(11, "notepad.exe", "Save As");
        modal.info.is_owned = true;
        modal.info.disables_owner = true;
        // Same PID — they belong to the same process instance.
        modal.pid = parent.pid;

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![parent, modal]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_closed().is_empty());
        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_out_of_process_modal_shields_owner() {
        // Some Win32 modals are hosted in a different process than their
        // owner (shell-hosted picker dialogs, security prompts). The shield
        // must cover the owner's PID, not just the dialog's.
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Close,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        let parent = make_entry(30, "notepad.exe", "Untitled - Notepad");
        // Modal lives in a different process (different pid) but reports the
        // parent's pid as owner_pid.
        let mut modal = make_entry(31, "PickerHost.exe", "Save As");
        modal.info.is_owned = true;
        modal.info.disables_owner = true;
        modal.info.owner_pid = Some(parent.pid);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![parent, modal]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        assert!(mock.get_closed().is_empty());
        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_parent_acted_on_with_floating_helper() {
        // An owned window that does NOT disable its parent (find/replace,
        // color picker, tool palette) must not shield the parent from idle
        // actions — Windows leaves the parent fully clickable.
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        let parent = make_entry(20, "notepad.exe", "Untitled - Notepad");
        let mut helper = make_entry(21, "notepad.exe", "Find");
        helper.info.is_owned = true;
        helper.info.disables_owner = false;
        helper.pid = parent.pid;

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![parent, helper]);
        timestamps.lock().unwrap().insert(
            "notepad.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        // Prime the idle clock so timeout=0 acts on the second poll.
        monitor.poll();
        monitor.poll();

        assert!(!mock.get_minimized().is_empty());
    }

    #[test]
    fn test_foreground_hwnd_skips_action() {
        // Two windows share a process name. The currently-foreground HWND must
        // not be acted on even though the process-name FG check could (e.g. if
        // get_foreground_process resolves to a different host's name).
        let config = make_config(
            vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _, timestamps) = setup(config);

        // Foreground process resolves to something different (simulating a
        // host process whose name differs from the window's own process name).
        mock.set_foreground(Some("other.exe"));
        mock.set_foreground_hwnd(42);
        mock.set_windows(vec![make_entry(42, "chrome.exe", "Active tab")]);
        timestamps.lock().unwrap().insert(
            "chrome.exe".to_string(),
            Instant::now() - Duration::from_secs(9999),
        );
        monitor.poll();

        // HWND-level check should have spared it.
        assert!(mock.get_minimized().is_empty());
    }

    #[test]
    fn test_external_timestamp_update_respected() {
        // Simulates the foreground hook updating timestamps externally
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 1,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
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
}
