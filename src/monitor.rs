/// Core idle-detection polling loop.
/// Tracks which processes were last in the foreground and triggers
/// minimize/close actions when idle timeouts are exceeded.

use crate::config::{Action, Config};
use crate::filter;
use crate::winapi::{WindowApi, WindowEntry};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Snapshot of a tracked window for the GUI's active-windows view.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ActiveWindowSnapshot {
    pub process: String,
    pub title: String,
    pub idle_secs: u64,
    pub managed: bool,
}

/// The monitor that runs the polling loop.
pub struct Monitor<W: WindowApi> {
    api: W,
    config: Arc<RwLock<Config>>,
    paused: Arc<AtomicBool>,
    /// Last time each process (lowercase) was the foreground window.
    last_foreground: HashMap<String, Instant>,
    /// HWND lookup: process_name_lower -> vec of (hwnd, title)
    /// Updated each poll cycle from enumeration.
    current_windows: Vec<WindowEntry>,
    /// Cumulative idle time per process (for "troublesome" ranking).
    cumulative_idle: HashMap<String, Duration>,
    /// Shared buffer for GUI to read active window snapshots.
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    /// Processes that had active rules last poll (lowercase).
    /// Used to detect newly managed processes and reset their idle clock.
    previously_managed: std::collections::HashSet<String>,
}

impl<W: WindowApi> Monitor<W> {
    pub fn new(
        api: W,
        config: Arc<RwLock<Config>>,
        paused: Arc<AtomicBool>,
        snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    ) -> Self {
        Self {
            api,
            config,
            paused,
            last_foreground: HashMap::new(),
            current_windows: Vec::new(),
            cumulative_idle: HashMap::new(),
            snapshot_buffer,
            previously_managed: std::collections::HashSet::new(),
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

        // 0. Detect newly managed processes and reset their idle clock.
        // This prevents immediate action when a rule is added for an already-open window.
        let mut currently_managed = std::collections::HashSet::new();
        for rule in &config.app_rule {
            if rule.enabled {
                currently_managed.insert(rule.process.to_lowercase());
            }
        }
        for bucket in &config.bucket {
            if bucket.enabled {
                for proc in &bucket.processes {
                    currently_managed.insert(proc.to_lowercase());
                }
            }
        }
        let mut newly_managed = std::collections::HashSet::new();
        for proc in &currently_managed {
            if !self.previously_managed.contains(proc) {
                // Newly managed — give it a fresh grace period
                self.last_foreground.insert(proc.clone(), now);
                newly_managed.insert(proc.clone());
            }
        }
        self.previously_managed = currently_managed;

        // 1. Update foreground process timestamp
        let foreground = self.api.get_foreground_process();
        if let Some(ref fg) = foreground {
            let fg_lower = fg.to_lowercase();
            self.last_foreground.insert(fg_lower, now);
        }

        // 2. Enumerate visible windows and store for GUI snapshot
        self.current_windows = self.api.enumerate_visible_windows();

        // 3-7. Check each window against rules and act
        // Collect actions first to avoid borrow conflicts.
        let fg_lower = foreground.as_deref().map(|s| s.to_lowercase());

        struct PendingAction {
            hwnd: isize,
            process: String,
            title: String,
            action: Action,
            idle_secs: f64,
        }

        let mut actions: Vec<PendingAction> = Vec::new();
        let mut new_timestamps: Vec<(String, Instant)> = Vec::new();
        let mut cumulative_updates: Vec<String> = Vec::new();

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

            // Skip processes that just became managed this poll cycle
            if newly_managed.contains(&proc_lower) {
                continue;
            }

            // Look up rule
            let rule = match config.resolve_process(&proc_lower) {
                Some(r) => r,
                None => {
                    cumulative_updates.push(proc_lower);
                    continue;
                }
            };

            // Check idle time
            let last_fg = self.last_foreground.get(&proc_lower).copied();
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
                log::debug!("Skipping fullscreen window: {} ({})", entry.info.process_name, entry.info.title);
                continue;
            }

            // Double-check window is still valid
            if !self.api.is_window_valid(entry.hwnd) {
                log::debug!("Window vanished: {} ({})", entry.info.process_name, entry.info.title);
                continue;
            }

            // Double-check it's not now the foreground
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
            });
            cumulative_updates.push(proc_lower);
        }

        // Apply deferred mutations
        for (proc, ts) in new_timestamps {
            self.last_foreground.insert(proc, ts);
        }
        for proc in cumulative_updates {
            self.track_cumulative_idle(&proc, now);
        }

        // Execute actions
        for action in actions {
            match action.action {
                Action::Minimize => {
                    log::info!("Minimizing: {} ({}) — idle {:.0}s", action.process, action.title, action.idle_secs);
                    self.api.minimize_window(action.hwnd);
                }
                Action::Close => {
                    log::info!("Closing: {} ({}) — idle {:.0}s", action.process, action.title, action.idle_secs);
                    self.api.close_window(action.hwnd);
                }
            }
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
        let config = match self.config.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let now = Instant::now();
        let mut snapshots: Vec<ActiveWindowSnapshot> = self
            .current_windows
            .iter()
            .filter(|e| !filter::is_system_window(&e.info))
            .filter(|e| !config.is_hidden(&e.info.process_name))
            .map(|e| {
                let proc_lower = e.info.process_name.to_lowercase();
                let idle_secs = self
                    .last_foreground
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

        // Sort by idle time descending (longest idle first)
        snapshots.sort_by(|a, b| b.idle_secs.cmp(&a.idle_secs));

        snapshots
    }

    fn track_cumulative_idle(&mut self, process: &str, now: Instant) {
        if let Some(last_fg) = self.last_foreground.get(process) {
            let idle = now.duration_since(*last_fg);
            let entry = self.cumulative_idle.entry(process.to_string()).or_default();
            *entry += idle;
        }
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
                .unwrap_or(30);

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
            info: WindowInfo {
                process_name: process.into(),
                title: title.into(),
                class_name: "AppWindow".into(),
                is_tool_window: false,
                is_owned: false,
                own_pid: false,
            },
        }
    }

    fn setup(
        config: Config,
    ) -> (Monitor<MockWindowApi>, MockWindowApi, Arc<RwLock<Config>>, Arc<AtomicBool>) {
        let mock = MockWindowApi::new();
        let config = Arc::new(RwLock::new(config));
        let paused = Arc::new(AtomicBool::new(false));
        let snapshot_buffer = Arc::new(Mutex::new(Vec::new()));
        let monitor = Monitor::new(mock.clone(), config.clone(), paused.clone(), snapshot_buffer);
        let mock_ref = mock.clone();
        (monitor, mock_ref, config, paused)
    }

    #[test]
    fn test_foreground_process_not_acted_on() {
        let config = make_config(
            vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 0, // immediate timeout
                action: Action::Minimize,
                enabled: true,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

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
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        // First poll: notepad is foreground, sets timestamp
        mock.set_foreground(Some("notepad.exe"));
        mock.set_windows(vec![make_entry(100, "notepad.exe", "Untitled")]);
        monitor.poll();

        // Second poll: something else is foreground, notepad has been idle
        // With timeout_mins=0, it should be acted on
        mock.set_foreground(Some("other.exe"));
        // Need to backdate the timestamp
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(1));
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
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        mock.set_foreground(Some("notepad.exe"));
        mock.set_windows(vec![make_entry(100, "notepad.exe", "Untitled")]);
        monitor.poll();

        mock.set_foreground(Some("other.exe"));
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(1));
        monitor.poll();

        assert!(!mock.get_closed().is_empty());
    }

    #[test]
    fn test_unmanaged_process_never_acted_on() {
        let config = make_config(vec![], vec![]);
        let (mut monitor, mock, _, _) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "unmanaged.exe", "Something")]);
        monitor
            .last_foreground
            .insert("unmanaged.exe".to_string(), Instant::now() - Duration::from_secs(9999));
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
            }],
            vec![],
        );
        let (mut monitor, mock, _, paused) = setup(config);

        paused.store(true, Ordering::Relaxed);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(9999));
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
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        // Chrome goes idle
        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        monitor
            .last_foreground
            .insert("chrome.exe".to_string(), Instant::now() - Duration::from_secs(30));
        monitor.poll();
        assert!(mock.get_minimized().is_empty()); // 30s < 60s timeout

        // Chrome comes back to foreground — timer resets
        mock.set_foreground(Some("chrome.exe"));
        monitor.poll();

        // Now check the timestamp was reset to ~now
        let ts = monitor.last_foreground.get("chrome.exe").unwrap();
        assert!(ts.elapsed().as_secs() < 2);
    }

    #[test]
    fn test_disabled_rule_not_acted_on() {
        let config = make_config(
            vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 0,
                action: Action::Minimize,
                enabled: false, // disabled
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(9999));
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
            }],
        );
        let (mut monitor, mock, _, _) = setup(config);

        mock.set_foreground(Some("chrome.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        monitor.poll();

        mock.set_foreground(Some("other.exe"));
        monitor
            .last_foreground
            .insert("chrome.exe".to_string(), Instant::now() - Duration::from_secs(1));
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
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![WindowEntry {
            hwnd: 1,
            info: WindowInfo {
                process_name: "dwm.exe".into(),
                title: "Desktop Window Manager".into(),
                class_name: "DWM".into(),
                is_tool_window: false,
                is_owned: false,
                own_pid: false,
            },
        }]);
        monitor
            .last_foreground
            .insert("dwm.exe".to_string(), Instant::now() - Duration::from_secs(9999));
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
            }],
            vec![],
        );
        let (mut monitor, mock, config_arc, _) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(60));
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
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        // First time seeing notepad — should set timestamp, not act
        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        // Do NOT pre-set last_foreground — this is first sighting
        monitor.poll();

        assert!(mock.get_minimized().is_empty());
        // Now the timestamp should be set
        assert!(monitor.last_foreground.contains_key("notepad.exe"));
    }

    #[test]
    fn test_active_windows_snapshot_sorted() {
        let config = make_config(
            vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
            }],
            vec![],
        );
        let (mut monitor, mock, _, _) = setup(config);

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![
            make_entry(1, "chrome.exe", "Google"),
            make_entry(2, "notepad.exe", "Untitled"),
        ]);

        // Set different idle times
        monitor
            .last_foreground
            .insert("chrome.exe".to_string(), Instant::now() - Duration::from_secs(300));
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(60));

        monitor.poll();

        let snapshot = monitor.get_active_windows_snapshot();
        assert!(snapshot.len() >= 1);
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
        let mut monitor = Monitor::new(
            mock.clone(),
            config_arc,
            paused,
            snapshot_buffer.clone(),
        );

        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "chrome.exe", "Google")]);
        monitor
            .last_foreground
            .insert("chrome.exe".to_string(), Instant::now() - Duration::from_secs(10));
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
        let mut monitor = Monitor::new(
            mock.clone(),
            config_arc,
            paused,
            snapshot_buffer.clone(),
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
        let (mut monitor, mock, config_arc, _) = setup(config);

        // notepad is open and was last in foreground a long time ago
        mock.set_foreground(Some("other.exe"));
        mock.set_windows(vec![make_entry(1, "notepad.exe", "Untitled")]);
        monitor
            .last_foreground
            .insert("notepad.exe".to_string(), Instant::now() - Duration::from_secs(9999));
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
            });
        }

        // First poll after rule added: should NOT minimize (grace period)
        monitor.poll();
        assert!(mock.get_minimized().is_empty());

        // Second poll: now it should act (idle clock was reset, but timeout=0)
        monitor.poll();
        assert!(!mock.get_minimized().is_empty());
    }
}
