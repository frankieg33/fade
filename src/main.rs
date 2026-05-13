// Suppress the console window on Windows GUI builds. Without this attribute the
// linker defaults to the `console` subsystem and every double-click of fade.exe
// pops a black cmd window. Logs that would have gone to stdout are routed to a
// file (see init_logger below) so they're still recoverable for debugging.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod autostart;
mod buckets;
mod config;
mod filter;
mod icon_catalog;
mod icons;
mod monitor;
mod tray;
mod winapi;

use config::{Action, Config};
use monitor::{ActiveWindowSnapshot, Monitor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use winapi::Win32Api;

slint::include_modules!();

/// Sentinel-property bumps cycle through this range. The Slint view turns
/// `redraw_tick` into an opacity (`tick * 0.0001 + 0.0001`), so values past
/// ~10k saturate and stop being treated as real changes. 1000 stays well
/// below saturation while every bump still produces a unique value.
const REDRAW_TICK_MODULO: i32 = 1000;

/// Delay before the follow-up repaint after `w.show()`. Long enough for
/// winit to have realized the window, short enough to be imperceptible.
const POST_SHOW_REPAINT_DELAY_MS: u64 = 50;

/// Bump the `redraw_tick` sentinel and ask the window to redraw. The
/// property change is what marks the window dirty for the Slint software
/// renderer — request_redraw on its own is not enough after a long hide.
fn force_repaint(w: &SettingsWindow) {
    // wrapping_add defends against the (unreachable but cheap-to-prevent)
    // case where the property was set out of band to a value near i32::MAX.
    w.set_redraw_tick(w.get_redraw_tick().wrapping_add(1) % REDRAW_TICK_MODULO);
    w.window().request_redraw();
}

/// Initialize the logger. In console builds (debug + non-Windows), env_logger
/// writes to stderr like before. In the Windows release build there is no
/// console attached, so logs are appended to `fade.log` next to the executable
/// (rotated by truncation when it exceeds 1 MB).
fn init_logger() {
    let level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let mut builder = env_logger::Builder::new();
    builder.parse_filters(&level).format_timestamp_millis();

    #[cfg(all(windows, not(debug_assertions)))]
    {
        if let Some(log_path) = log_file_path() {
            // Truncate if too big (cheap rotation; users rarely need >1 MB).
            if let Ok(meta) = std::fs::metadata(&log_path) {
                if meta.len() > 1_000_000 {
                    let _ = std::fs::remove_file(&log_path);
                }
            }
            if let Ok(file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                builder.target(env_logger::Target::Pipe(Box::new(file)));
            }
        }
    }

    let _ = builder.try_init();
}

#[cfg(all(windows, not(debug_assertions)))]
fn log_file_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.with_file_name("fade.log"))
}

fn main() {
    init_logger();

    log::info!("Fade v{} starting", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = Config::load();
    let config = Arc::new(RwLock::new(config));
    let paused = Arc::new(AtomicBool::new(false));
    let should_stop = Arc::new(AtomicBool::new(false));
    let snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>> = Arc::new(Mutex::new(Vec::new()));
    let foreground_timestamps: monitor::ForegroundTimestamps =
        Arc::new(Mutex::new(std::collections::HashMap::new()));
    let action_log: monitor::ActionLog = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let window_visible = Arc::new(AtomicBool::new(false));

    // Create Slint window (but don't show it yet)
    let window = match SettingsWindow::new() {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create settings window: {}", e);
            // Continue headless — tray only
            run_headless(config, paused, should_stop, action_log);
            return;
        }
    };

    // Shared search-query state, filters what update_gui_from_config renders.
    let search_state: Arc<RwLock<String>> = Arc::new(RwLock::new(String::new()));

    // Restore window geometry from config
    if let Ok(cfg) = config.read() {
        if let (Some(w), Some(h)) = (cfg.general.window_width, cfg.general.window_height) {
            window.window().set_size(slint::PhysicalSize::new(w, h));
        }
        if let (Some(x), Some(y)) = (cfg.general.window_x, cfg.general.window_y) {
            window
                .window()
                .set_position(slint::PhysicalPosition::new(x, y));
        }
    }

    // Populate GUI from config. If either lock is poisoned (e.g. a previous
    // panicking thread held them) keep the daemon alive with safe fallbacks
    // rather than crashing the tray.
    {
        let cfg = config.read().map(|g| (*g).clone()).unwrap_or_else(|_| {
            log::error!("Config lock poisoned on startup — using defaults for initial GUI");
            Config::default_config()
        });
        let query = search_state.read().map(|s| s.clone()).unwrap_or_default();
        update_gui_from_config(&window, &cfg, &query);
    }

    // Wire up GUI callbacks
    setup_gui_callbacks(
        &window,
        config.clone(),
        snapshot_buffer.clone(),
        search_state.clone(),
    );

    // Close hides to tray — does NOT quit, and persists window geometry.
    let window_weak = window.as_weak();
    let visible_for_close = window_visible.clone();
    let config_for_close = config.clone();
    window.window().on_close_requested(move || {
        if let Some(w) = window_weak.upgrade() {
            let size = w.window().size();
            let pos = w.window().position();
            if let Ok(mut c) = config_for_close.write() {
                c.general.window_width = Some(size.width);
                c.general.window_height = Some(size.height);
                c.general.window_x = Some(pos.x);
                c.general.window_y = Some(pos.y);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
            }
            w.hide().ok();
        }
        visible_for_close.store(false, Ordering::Relaxed);
        slint::CloseRequestResponse::KeepWindowShown
    });

    // Load app icon for tray
    let (icon_rgba, icon_w, icon_h) = tray::load_icon();

    // Create tray icon
    let tray_result = tray::create_tray_icon(icon_rgba, icon_w, icon_h);

    let _tray_icon = match tray_result {
        Ok(tray) => {
            log::info!("Tray icon created");
            Some(tray)
        }
        Err(e) => {
            log::error!("Failed to create tray icon: {}", e);
            // Show the settings window since there's no tray
            window.show().ok();
            None
        }
    };

    // Poll tray events via Slint timer
    let window_weak = window.as_weak();
    let paused_for_tray = paused.clone();
    let config_for_tray = config.clone();
    let search_for_tray = search_state.clone();
    let log_for_tray = action_log.clone();
    let visible_for_tray = window_visible.clone();
    let tray_timer = slint::Timer::default();
    tray_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        move || match tray::poll_tray_events() {
            tray::TrayAction::ShowSettings => {
                if let Some(w) = window_weak.upgrade() {
                    if let Ok(cfg) = config_for_tray.read() {
                        let q = search_for_tray
                            .read()
                            .map(|s| s.clone())
                            .unwrap_or_default();
                        update_gui_from_config(&w, &cfg, &q);
                        refresh_activity_log(&w, &cfg, &log_for_tray);
                    }
                    w.show().ok();
                    // A long-hidden Slint window backed by the software
                    // renderer can paint a stale framebuffer on the first
                    // frame after show — repaint now, and again once winit
                    // has finished realizing the window.
                    force_repaint(&w);
                    let weak = w.as_weak();
                    slint::Timer::single_shot(
                        std::time::Duration::from_millis(POST_SHOW_REPAINT_DELAY_MS),
                        move || {
                            if let Some(w) = weak.upgrade() {
                                force_repaint(&w);
                            }
                        },
                    );
                    visible_for_tray.store(true, Ordering::Relaxed);
                }
            }
            tray::TrayAction::TogglePause => {
                let was_paused = paused_for_tray.load(Ordering::Relaxed);
                let new_state = !was_paused;
                paused_for_tray.store(new_state, Ordering::Relaxed);
                if let Some(w) = window_weak.upgrade() {
                    w.set_paused(new_state);
                }
                log::info!(
                    "Monitoring {}",
                    if was_paused { "resumed" } else { "paused" }
                );
            }
            tray::TrayAction::Quit => {
                log::info!("Quit requested from tray");
                slint::quit_event_loop().ok();
            }
            tray::TrayAction::None => {}
        },
    );

    // Refresh active windows + activity log from the monitor's shared buffers
    let snapshot_for_gui = snapshot_buffer.clone();
    let config_for_gui = config.clone();
    let visible_for_gui = window_visible.clone();
    let log_for_gui = action_log.clone();
    let gui_weak = window.as_weak();
    let gui_refresh_timer = slint::Timer::default();
    gui_refresh_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(2),
        move || {
            if !visible_for_gui.load(Ordering::Relaxed) {
                return;
            }
            if let Some(w) = gui_weak.upgrade() {
                if let Ok(cfg) = config_for_gui.read() {
                    refresh_active_windows(&w, &cfg, &snapshot_for_gui);
                    refresh_current_apps(&w, &cfg, &snapshot_for_gui);
                    refresh_activity_log(&w, &cfg, &log_for_gui);
                }
                force_repaint(&w);
            }
        },
    );

    // Install foreground event hook (updates timestamps in real-time via Win32 callback)
    let _foreground_hook = match winapi::install_foreground_hook(foreground_timestamps.clone()) {
        Ok(guard) => Some(guard),
        Err(e) => {
            log::warn!(
                "Failed to install foreground hook, falling back to polling: {}",
                e
            );
            None
        }
    };

    // Spawn monitor thread
    let monitor_config = config.clone();
    let monitor_paused = paused.clone();
    let monitor_stop = should_stop.clone();
    let monitor_snapshot = snapshot_buffer.clone();
    let monitor_timestamps = foreground_timestamps.clone();
    let monitor_log = action_log.clone();
    let monitor_thread = std::thread::spawn(move || {
        let api = Win32Api::new();
        let mut monitor = Monitor::new(
            api,
            monitor_config,
            monitor_paused,
            monitor_snapshot,
            monitor_timestamps,
            monitor_log,
        );
        monitor.run(monitor_stop);
    });

    // Show the settings window on launch (user opt-in to close-to-tray behavior).
    window.show().ok();
    window_visible.store(true, Ordering::Relaxed);

    // Run Slint event loop (blocks until quit)
    // Don't quit when last window is hidden
    let _ = slint::run_event_loop_until_quit();

    // Cleanup
    log::info!("Shutting down...");
    should_stop.store(true, Ordering::Relaxed);
    if monitor_thread.join().is_err() {
        log::error!("Monitor thread panicked");
    }

    // Save config on exit
    if let Ok(cfg) = config.read() {
        if let Err(e) = cfg.save() {
            log::error!("Failed to save config on exit: {}", e);
        }
    }

    log::info!("Fade stopped");
}

/// Check if a process exists in any bucket.
fn process_in_any_bucket(config: &Config, process: &str) -> bool {
    let lower = process.to_lowercase();
    config
        .bucket
        .iter()
        .any(|b| b.processes.iter().any(|p| p.to_lowercase() == lower))
}

/// Find the app_rule for a process, if any.
fn find_app_rule<'a>(config: &'a Config, process: &str) -> Option<&'a config::AppRule> {
    let lower = process.to_lowercase();
    config
        .app_rule
        .iter()
        .find(|r| r.process.to_lowercase() == lower)
}

/// Build GroupModel list from config buckets.
fn build_groups(config: &Config, search: &str) -> Vec<GroupModel> {
    let q = search.trim().to_lowercase();
    config
        .bucket
        .iter()
        .enumerate()
        .filter_map(|(g_idx, bucket)| {
            let name_matches = q.is_empty() || bucket.name.to_lowercase().contains(&q);
            let apps: Vec<GroupAppModel> = bucket
                .processes
                .iter()
                .enumerate()
                .filter(|(_, proc)| {
                    q.is_empty() || name_matches || proc.to_lowercase().contains(&q)
                })
                .map(|(a_idx, proc)| {
                    let rule = find_app_rule(config, proc);
                    let customized = rule
                        .map(|r| config::app_is_customized(bucket, r))
                        .unwrap_or(false);
                    let (enabled, timeout, action) = if let Some(r) = rule {
                        (r.enabled, r.timeout_mins as i32, r.action.as_str().into())
                    } else {
                        (
                            bucket.enabled,
                            bucket.timeout_mins as i32,
                            bucket.action.as_str().into(),
                        )
                    };
                    GroupAppModel {
                        icon: config.icon_for_app(proc).into(),
                        process: proc.clone().into(),
                        customized,
                        enabled,
                        timeout_mins: timeout,
                        action,
                        orig_app_index: a_idx as i32,
                    }
                })
                .collect();

            // Drop group entirely if search is non-empty and neither the name nor any app matched.
            if !q.is_empty() && !name_matches && apps.is_empty() {
                return None;
            }

            Some(GroupModel {
                icon: config.icon_for_bucket(g_idx).into(),
                name: bucket.name.clone().into(),
                enabled: bucket.enabled,
                timeout_mins: bucket.timeout_mins as i32,
                action: bucket.action.as_str().into(),
                apps: std::rc::Rc::new(slint::VecModel::from(apps)).into(),
                // Force-expand while a search is active so matches are visible.
                // Original collapse state is preserved in config and restored when search clears.
                expanded: if q.is_empty() { bucket.expanded } else { true },
                orig_index: g_idx as i32,
            })
        })
        .collect()
}

/// Build unassigned rules — app_rules whose process isn't in any bucket.
fn build_unassigned_rules(config: &Config, search: &str) -> Vec<UnassignedRuleModel> {
    let q = search.trim().to_lowercase();
    // Pre-compute the unfiltered unassigned ordering so the orig_index we
    // expose matches what the Rust callbacks reconstruct on dispatch.
    let unfiltered: Vec<&config::AppRule> = config
        .app_rule
        .iter()
        .filter(|r| !process_in_any_bucket(config, &r.process))
        .collect();
    unfiltered
        .iter()
        .enumerate()
        .filter(|(_, r)| q.is_empty() || r.process.to_lowercase().contains(&q))
        .map(|(idx, r)| UnassignedRuleModel {
            icon: config.icon_for_app(&r.process).into(),
            process: r.process.clone().into(),
            timeout_mins: r.timeout_mins as i32,
            action: r.action.as_str().into(),
            enabled: r.enabled,
            orig_index: idx as i32,
        })
        .collect()
}

/// Count total managed apps (enabled bucket apps + enabled unassigned rules).
fn count_managed(config: &Config) -> i32 {
    let bucket_count: usize = config
        .bucket
        .iter()
        .filter(|b| b.enabled)
        .map(|b| {
            b.processes
                .iter()
                .filter(|p| {
                    // Count if no custom rule, OR if custom rule is enabled
                    match find_app_rule(config, p) {
                        Some(rule) => rule.enabled,
                        None => true,
                    }
                })
                .count()
        })
        .sum();
    let unassigned_count = config
        .app_rule
        .iter()
        .filter(|r| r.enabled && !process_in_any_bucket(config, &r.process))
        .count();
    (bucket_count + unassigned_count) as i32
}

/// Populate Slint GUI properties from the Config struct.
fn update_gui_from_config(window: &SettingsWindow, config: &Config, search: &str) {
    let groups = build_groups(config, search);
    window.set_groups(std::rc::Rc::new(slint::VecModel::from(groups)).into());

    let unassigned = build_unassigned_rules(config, search);
    window.set_unassigned_rules(std::rc::Rc::new(slint::VecModel::from(unassigned)).into());

    window.set_managed_count(count_managed(config));
    window.set_polling_interval_secs(config.general.polling_interval_secs as i32);
    window.set_auto_start(config.general.auto_start);
    if let Some(h) = config.general.activity_current_height {
        window.set_activity_current_height(h);
    }
    window.set_version(env!("CARGO_PKG_VERSION").into());
}

fn format_relative_time(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if ts == 0 || ts > now {
        return "just now".into();
    }
    let age = now - ts;
    if age < 60 {
        format!("{}s ago", age)
    } else if age < 3600 {
        format!("{}m ago", age / 60)
    } else if age < 86400 {
        format!("{}h ago", age / 3600)
    } else {
        format!("{}d ago", age / 86400)
    }
}

/// Refresh the Activity page model from the monitor's action log.
fn refresh_activity_log(window: &SettingsWindow, config: &Config, log: &monitor::ActionLog) {
    if let Ok(entries) = log.lock() {
        let models: Vec<ActivityLogModel> = entries
            .iter()
            .map(|e| ActivityLogModel {
                icon: config.icon_for_app(&e.process).into(),
                process: e.process.clone().into(),
                action: e.action.as_str().into(),
                time_str: format_relative_time(e.timestamp).into(),
                title: e.title.clone().into(),
            })
            .collect();
        window.set_activity_log(std::rc::Rc::new(slint::VecModel::from(models)).into());
    }
}

/// Refresh the active processes in the drawer + active count.
fn refresh_active_windows(
    window: &SettingsWindow,
    config: &Config,
    snapshot_buffer: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
) {
    if let Ok(buf) = snapshot_buffer.lock() {
        // Deduplicate by process name (keep first occurrence)
        let mut seen = std::collections::HashSet::new();
        let models: Vec<ActiveProcessModel> = buf
            .iter()
            .filter(|s| !config.is_hidden(&s.process))
            .filter(|s| seen.insert(s.process.to_lowercase()))
            .map(|s| ActiveProcessModel {
                icon: icons::process_icon(&s.process).into(),
                process: s.process.clone().into(),
                managed: config.resolve_process(&s.process).is_some(),
            })
            .collect();
        window.set_active_count(models.len() as i32);
        window.set_active_processes(std::rc::Rc::new(slint::VecModel::from(models)).into());
    }
}

fn format_idle_duration(secs: u64) -> String {
    if secs < 10 {
        "active now".to_string()
    } else if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h ago", h)
        } else {
            format!("{}h {}m ago", h, m)
        }
    }
}

/// Refresh the "Currently open" list on the Activity page.
fn refresh_current_apps(
    window: &SettingsWindow,
    config: &Config,
    snapshot_buffer: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
) {
    if let Ok(buf) = snapshot_buffer.lock() {
        let mut seen = std::collections::HashSet::new();
        let models: Vec<CurrentAppModel> = buf
            .iter()
            .filter(|s| !config.is_hidden(&s.process))
            .filter(|s| seen.insert(s.process.to_lowercase()))
            .map(|s| CurrentAppModel {
                icon: config.icon_for_app(&s.process).into(),
                process_name: s.process.clone().into(),
                idle_str: format_idle_duration(s.idle_secs).into(),
                managed: config.resolve_process(&s.process).is_some(),
            })
            .collect();
        window.set_current_apps(std::rc::Rc::new(slint::VecModel::from(models)).into());
    }
}

/// Wire Slint callbacks to modify the shared config.
/// Full refresh helper — reads current search state and repopulates all GUI properties.
fn do_refresh_all(
    weak: &slint::Weak<SettingsWindow>,
    cfg: &Config,
    snap: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    search_state: &Arc<RwLock<String>>,
) {
    if let Some(w) = weak.upgrade() {
        let q = search_state.read().map(|s| s.clone()).unwrap_or_default();
        update_gui_from_config(&w, cfg, &q);
        refresh_active_windows(&w, cfg, snap);
        refresh_current_apps(&w, cfg, snap);
    }
}

fn setup_gui_callbacks(
    window: &SettingsWindow,
    config: Arc<RwLock<Config>>,
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
    search_state: Arc<RwLock<String>>,
) {
    // ── Group (bucket) callbacks ──

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_toggle_group(move |idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].enabled = enabled;
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    let cfg = config.clone();
    window.on_update_group_timeout(move |idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].timeout_mins = mins as u64;
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_group_action(move |idx, action| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].action = Action::from_str(&action);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_toggle_group_expanded(move |idx| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].expanded = !c.bucket[idx].expanded;
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    // Populate icon picker with full catalog on startup.
    window.set_icon_results(
        std::rc::Rc::new(slint::VecModel::from(
            icon_catalog::CATALOG
                .iter()
                .map(|e| IconEntry {
                    glyph: e.glyph.into(),
                    keywords: e.keywords.into(),
                })
                .collect::<Vec<_>>(),
        ))
        .into(),
    );

    let weak = window.as_weak();
    window.on_icon_search_changed(move |query| {
        let results: Vec<IconEntry> = icon_catalog::search(&query)
            .into_iter()
            .map(|e| IconEntry {
                glyph: e.glyph.into(),
                keywords: e.keywords.into(),
            })
            .collect();
        if let Some(w) = weak.upgrade() {
            w.set_icon_results(std::rc::Rc::new(slint::VecModel::from(results)).into());
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_set_group_icon(move |g_idx, glyph| {
        if let Ok(mut c) = cfg.write() {
            let idx = g_idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].icon = Some(glyph.to_string());
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_set_app_icon(move |g_idx, a_idx, glyph| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g >= c.bucket.len() || a >= c.bucket[g].processes.len() {
                return;
            }
            let process = c.bucket[g].processes[a].clone();
            let process_lower = process.to_lowercase();
            // Find-or-create an AppRule so we can attach the icon.
            if let Some(rule) = c
                .app_rule
                .iter_mut()
                .find(|r| r.process.to_lowercase() == process_lower)
            {
                rule.icon = Some(glyph.to_string());
            } else {
                let bucket_timeout = c.bucket[g].timeout_mins;
                let bucket_action = c.bucket[g].action.clone();
                // Inherit the bucket's enabled state. Hard-coding enabled=true
                // would silently promote an app inside a disabled bucket into a
                // managed app just because the user picked a custom icon.
                let bucket_enabled = c.bucket[g].enabled;
                c.app_rule.push(config::AppRule {
                    process,
                    timeout_mins: bucket_timeout,
                    action: bucket_action,
                    enabled: bucket_enabled,
                    icon: Some(glyph.to_string()),
                    customized: false,
                });
            }
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_move_app(move |from_g, app_idx, to_g| {
        if let Ok(mut c) = cfg.write() {
            let from = from_g as usize;
            let to = to_g as usize;
            let a = app_idx as usize;
            if from == to {
                return;
            }
            if from >= c.bucket.len() || to >= c.bucket.len() {
                return;
            }
            if a >= c.bucket[from].processes.len() {
                return;
            }
            let process = c.bucket[from].processes.remove(a);
            let already_there = c.bucket[to]
                .processes
                .iter()
                .any(|p| p.eq_ignore_ascii_case(&process));
            if !already_there {
                c.bucket[to].processes.push(process);
            }
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_remove_from_group(move |g_idx, app_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = app_idx as usize;
            if g >= c.bucket.len() || a >= c.bucket[g].processes.len() {
                return;
            }
            let proc = c.bucket[g].processes[a].clone();
            let bucket_timeout = c.bucket[g].timeout_mins;
            let bucket_action = c.bucket[g].action.clone();
            let bucket_enabled = c.bucket[g].enabled;
            c.bucket[g].processes.remove(a);
            // Ensure an AppRule exists so the app lands in Ungrouped (not nowhere).
            if !c
                .app_rule
                .iter()
                .any(|r| r.process.eq_ignore_ascii_case(&proc))
            {
                c.app_rule.push(config::AppRule {
                    process: proc,
                    timeout_mins: bucket_timeout,
                    action: bucket_action,
                    enabled: bucket_enabled,
                    icon: None,
                    customized: false,
                });
            }
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_delete_app_from_group(move |g_idx, app_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = app_idx as usize;
            if g >= c.bucket.len() || a >= c.bucket[g].processes.len() {
                return;
            }
            let proc = c.bucket[g].processes.remove(a);
            // Also purge any standalone AppRule for this process.
            c.app_rule
                .retain(|r| !r.process.eq_ignore_ascii_case(&proc));
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_move_app_up(move |g_idx, app_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = app_idx as usize;
            if g >= c.bucket.len() || a == 0 || a >= c.bucket[g].processes.len() {
                return;
            }
            c.bucket[g].processes.swap(a, a - 1);
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_move_app_down(move |g_idx, app_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = app_idx as usize;
            if g >= c.bucket.len() || a + 1 >= c.bucket[g].processes.len() {
                return;
            }
            c.bucket[g].processes.swap(a, a + 1);
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_add_to_group(move |g_idx, process| {
        let name = process.trim().to_string();
        if name.is_empty() {
            return;
        }
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            if g >= c.bucket.len() {
                return;
            }
            let already = c.bucket[g]
                .processes
                .iter()
                .any(|p| p.eq_ignore_ascii_case(&name));
            if !already {
                c.bucket[g].processes.push(name);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_create_group(move || {
        if let Ok(mut c) = cfg.write() {
            let base = "New Group";
            let mut n = 1;
            let mut name = base.to_string();
            while c.bucket.iter().any(|b| b.name == name) {
                n += 1;
                name = format!("{} {}", base, n);
            }
            c.bucket.push(config::Bucket {
                name,
                processes: Vec::new(),
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: false,
                expanded: true,
                icon: None,
            });
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_delete_group(move |idx| {
        if let Ok(mut c) = cfg.write() {
            let i = idx as usize;
            if i < c.bucket.len() {
                c.bucket.remove(i);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_move_to_new_group(move |from_g, app_idx| {
        if let Ok(mut c) = cfg.write() {
            let from = from_g as usize;
            let a = app_idx as usize;
            if from >= c.bucket.len() || a >= c.bucket[from].processes.len() {
                return;
            }
            let process = c.bucket[from].processes.remove(a);
            let base = "New Group";
            let mut n = 1;
            let mut name = base.to_string();
            while c.bucket.iter().any(|b| b.name == name) {
                n += 1;
                name = format!("{} {}", base, n);
            }
            c.bucket.push(config::Bucket {
                name,
                processes: vec![process],
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: false,
                expanded: true,
                icon: None,
            });
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_assign_to_new_group(move |u_idx| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c
                .app_rule
                .iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.clone())
                .collect();
            let Some(proc) = unassigned_processes.get(u_idx as usize).cloned() else {
                return;
            };
            let base = "New Group";
            let mut n = 1;
            let mut name = base.to_string();
            while c.bucket.iter().any(|b| b.name == name) {
                n += 1;
                name = format!("{} {}", base, n);
            }
            c.bucket.push(config::Bucket {
                name,
                processes: vec![proc],
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: false,
                expanded: true,
                icon: None,
            });
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_rename_group(move |idx, new_name| {
        let trimmed = new_name.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].name = trimmed;
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_add_app_to_group(move |g_idx, process| {
        let process_str = process.to_string();
        if process_str.is_empty() {
            return;
        }
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            if g < c.bucket.len() {
                let already = c.bucket[g]
                    .processes
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(&process_str));
                if !already {
                    c.bucket[g].processes.push(process_str);
                    if let Err(e) = c.save() {
                        log::error!("config save failed: {}", e);
                    }
                    do_refresh_all(&weak, &c, &snap, &search);
                }
            }
        }
    });

    // ── App-in-group callbacks ──

    // Customize: create an app_rule for a bucket process (copies bucket settings as starting point)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_customize_app(move |g_idx, a_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process = c.bucket[g].processes[a].clone();
                let process_lower = process.to_lowercase();
                let timeout_mins = c.bucket[g].timeout_mins;
                let action = c.bucket[g].action.clone();
                // Flip an existing (icon-only) rule to customized, or create a new one.
                if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == process_lower)
                {
                    rule.customized = true;
                } else {
                    c.app_rule.push(config::AppRule {
                        process,
                        timeout_mins,
                        action,
                        enabled: true,
                        icon: None,
                        customized: true,
                    });
                }
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    // Reset to group: delete the app_rule for this bucket process
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_reset_app_to_group(move |g_idx, a_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process_lower = c.bucket[g].processes[a].to_lowercase();
                c.app_rule
                    .retain(|r| r.process.to_lowercase() != process_lower);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    let cfg = config.clone();
    window.on_update_app_timeout(move |g_idx, a_idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process_lower = c.bucket[g].processes[a].to_lowercase();
                if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == process_lower)
                {
                    rule.timeout_mins = mins as u64;
                    if let Err(e) = c.save() {
                        log::error!("config save failed: {}", e);
                    }
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_app_action(move |g_idx, a_idx, action| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process_lower = c.bucket[g].processes[a].to_lowercase();
                if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == process_lower)
                {
                    rule.action = Action::from_str(&action);
                    if let Err(e) = c.save() {
                        log::error!("config save failed: {}", e);
                    }
                }
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_toggle_app(move |g_idx, a_idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process = c.bucket[g].processes[a].clone();
                let process_lower = process.to_lowercase();
                let bucket_timeout = c.bucket[g].timeout_mins;
                let bucket_action = c.bucket[g].action.clone();
                // If toggling an inherited app, create a custom rule first
                let exists = c
                    .app_rule
                    .iter()
                    .any(|r| r.process.to_lowercase() == process_lower);
                if !exists {
                    c.app_rule.push(config::AppRule {
                        process,
                        timeout_mins: bucket_timeout,
                        action: bucket_action,
                        enabled,
                        icon: None,
                        customized: false,
                    });
                } else if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == process_lower)
                {
                    rule.enabled = enabled;
                }
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    // ── Unassigned rule callbacks ──

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_assign_to_group({
        let cfg = config.clone();
        let weak = window.as_weak();
        let snap = snapshot_buffer.clone();
        let search = search_state.clone();
        move |u_idx, g_idx| {
            if let Ok(mut c) = cfg.write() {
                let g = g_idx as usize;
                if g >= c.bucket.len() {
                    return;
                }
                // Identify unassigned rule by index at call time.
                let unassigned_processes: Vec<String> = c
                    .app_rule
                    .iter()
                    .filter(|r| !process_in_any_bucket(&c, &r.process))
                    .map(|r| r.process.clone())
                    .collect();
                if let Some(proc) = unassigned_processes.get(u_idx as usize) {
                    let proc = proc.clone();
                    let already = c.bucket[g]
                        .processes
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case(&proc));
                    if !already {
                        c.bucket[g].processes.push(proc);
                        if let Err(e) = c.save() {
                            log::error!("config save failed: {}", e);
                        }
                        do_refresh_all(&weak, &c, &snap, &search);
                    }
                }
            }
        }
    });

    window.on_remove_unassigned(move |idx| {
        if let Ok(mut c) = cfg.write() {
            // Find the idx-th unassigned rule
            let unassigned_processes: Vec<String> = c
                .app_rule
                .iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                let proc = proc.clone();
                c.app_rule.retain(|r| r.process.to_lowercase() != proc);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                do_refresh_all(&weak, &c, &snap, &search);
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_toggle_unassigned(move |idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c
                .app_rule
                .iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == *proc)
                {
                    rule.enabled = enabled;
                    if let Err(e) = c.save() {
                        log::error!("config save failed: {}", e);
                    }
                    do_refresh_all(&weak, &c, &snap, &search);
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_unassigned_timeout(move |idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c
                .app_rule
                .iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == *proc)
                {
                    rule.timeout_mins = mins as u64;
                    if let Err(e) = c.save() {
                        log::error!("config save failed: {}", e);
                    }
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_unassigned_action(move |idx, action| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c
                .app_rule
                .iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                if let Some(rule) = c
                    .app_rule
                    .iter_mut()
                    .find(|r| r.process.to_lowercase() == *proc)
                {
                    rule.action = Action::from_str(&action);
                    if let Err(e) = c.save() {
                        log::error!("config save failed: {}", e);
                    }
                }
            }
        }
    });

    // ── Drawer callbacks ──

    // add-rule: add as unassigned app_rule (from active process drawer)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_add_rule(move |process| {
        let process_str = process.to_string();
        let Some(w) = weak.upgrade() else {
            return;
        };
        if process_str.is_empty() {
            w.set_drawer_status_kind(3);
            w.set_drawer_status_text("Enter a process name".into());
            return;
        }
        if let Ok(mut c) = cfg.write() {
            if c.app_rule
                .iter()
                .any(|r| r.process.eq_ignore_ascii_case(&process_str))
                || process_in_any_bucket(&c, &process_str)
            {
                w.set_drawer_status_kind(2);
                w.set_drawer_status_text(format!("Already added: {}", process_str).into());
                return;
            }
            c.app_rule.push(config::AppRule {
                process: process_str.clone(),
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            });
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
            w.set_drawer_status_kind(1);
            w.set_drawer_status_text(format!("Added: {}", process_str).into());
        }
    });

    // add-process-name: same as add-rule (manual text entry)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_add_process_name(move |process| {
        let process_str = process.to_string();
        let trimmed = process_str.trim().to_string();
        let Some(w) = weak.upgrade() else {
            return;
        };
        if trimmed.is_empty() {
            w.set_drawer_status_kind(3);
            w.set_drawer_status_text("Enter a process name".into());
            return;
        }
        // Very loose "looks like a .exe or a process name" check.
        if !trimmed.contains('.') {
            w.set_drawer_status_kind(3);
            w.set_drawer_status_text(
                format!("'{}' must include an extension (e.g. .exe)", trimmed).into(),
            );
            return;
        }
        if let Ok(mut c) = cfg.write() {
            if c.app_rule
                .iter()
                .any(|r| r.process.eq_ignore_ascii_case(&trimmed))
                || process_in_any_bucket(&c, &trimmed)
            {
                w.set_drawer_status_kind(2);
                w.set_drawer_status_text(format!("Already added: {}", trimmed).into());
                return;
            }
            c.app_rule.push(config::AppRule {
                process: trimmed.clone(),
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            });
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
            w.set_drawer_status_kind(1);
            w.set_drawer_status_text(format!("Added: {}", trimmed).into());
        }
    });

    // ── General settings ──

    let cfg = config.clone();
    window.on_set_polling_interval(move |secs| {
        if let Ok(mut c) = cfg.write() {
            c.general.polling_interval_secs = secs as u64;
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
        }
    });

    let cfg = config.clone();
    window.on_set_activity_current_height(move |h| {
        // The UI drag clamps to 80..800 already; defend against a non-finite
        // value sneaking in (saturated arithmetic, etc.) before it touches
        // persisted config or future loads.
        if !h.is_finite() {
            return;
        }
        let clamped = h.clamp(80.0, 800.0);
        if let Ok(mut c) = cfg.write() {
            c.general.activity_current_height = Some(clamped);
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
        }
    });

    let cfg = config.clone();
    window.on_set_auto_start(move |enabled| {
        if let Err(e) = autostart::set_auto_start(enabled) {
            log::error!("Auto-start toggle failed: {}", e);
            return;
        }
        if let Ok(mut c) = cfg.write() {
            c.general.auto_start = enabled;
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
        }
    });

    // hide-process (kept for potential future use)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_hide_process(move |process| {
        if let Ok(mut c) = cfg.write() {
            let process_str = process.to_string();
            if !c.general.hidden_processes.contains(&process_str) {
                c.general.hidden_processes.push(process_str);
                if let Err(e) = c.save() {
                    log::error!("config save failed: {}", e);
                }
                if let Some(w) = weak.upgrade() {
                    refresh_active_windows(&w, &c, &snap);
                }
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_search_changed(move |query| {
        if let Ok(mut s) = search.write() {
            *s = query.to_string();
        }
        if let Ok(c) = cfg.read() {
            do_refresh_all(&weak, &c, &snap, &search);
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    let search = search_state.clone();
    window.on_restore_defaults(move || {
        // Defaults set auto_start=false; that field alone does not unregister
        // the Windows startup entry, so we must call set_auto_start(false) too.
        // Otherwise the UI/config claim Fade won't start with Windows while the
        // OS keeps launching it.
        if let Err(e) = autostart::set_auto_start(false) {
            log::warn!("Auto-start disable during restore failed: {}", e);
        }
        if let Ok(mut c) = cfg.write() {
            *c = Config::default_config();
            if let Err(e) = c.save() {
                log::error!("config save failed: {}", e);
            }
            do_refresh_all(&weak, &c, &snap, &search);
        }
        log::info!("Config restored to defaults");
    });
}

/// Fallback: run without GUI if Slint window creation fails.
fn run_headless(
    config: Arc<RwLock<Config>>,
    paused: Arc<AtomicBool>,
    should_stop: Arc<AtomicBool>,
    action_log: monitor::ActionLog,
) {
    log::warn!("Running in headless mode (no GUI)");
    let api = Win32Api::new();
    let dummy_buffer = Arc::new(Mutex::new(Vec::new()));
    let foreground_timestamps = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let mut monitor = Monitor::new(
        api,
        config,
        paused,
        dummy_buffer,
        foreground_timestamps,
        action_log,
    );
    monitor.run(should_stop);
}
