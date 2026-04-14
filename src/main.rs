mod autostart;
mod buckets;
mod config;
mod filter;
mod monitor;
mod tray;
mod winapi;

use config::{Action, Config};
use monitor::{ActiveWindowSnapshot, Monitor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use winapi::Win32Api;

slint::include_modules!();

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    log::info!("Fade v{} starting", env!("CARGO_PKG_VERSION"));

    // Load config
    let config = Config::load();
    let config = Arc::new(RwLock::new(config));
    let paused = Arc::new(AtomicBool::new(false));
    let should_stop = Arc::new(AtomicBool::new(false));
    let snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>> =
        Arc::new(Mutex::new(Vec::new()));
    let window_visible = Arc::new(AtomicBool::new(false));

    // Create Slint window (but don't show it yet)
    let window = match SettingsWindow::new() {
        Ok(w) => w,
        Err(e) => {
            log::error!("Failed to create settings window: {}", e);
            // Continue headless — tray only
            run_headless(config, paused, should_stop);
            return;
        }
    };

    // Populate GUI from config
    update_gui_from_config(&window, &config.read().unwrap());

    // Wire up GUI callbacks
    setup_gui_callbacks(&window, config.clone(), snapshot_buffer.clone());

    // Close hides to tray — does NOT quit
    let window_weak = window.as_weak();
    let visible_for_close = window_visible.clone();
    window.window().on_close_requested(move || {
        if let Some(w) = window_weak.upgrade() {
            w.hide().ok();
        }
        visible_for_close.store(false, Ordering::Relaxed);
        slint::CloseRequestResponse::KeepWindowShown
    });

    // Create tray icon
    let (icon_rgba, icon_w, icon_h) = tray::generate_default_icon();
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
    let visible_for_tray = window_visible.clone();
    let tray_timer = slint::Timer::default();
    tray_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        move || {
            match tray::poll_tray_events() {
                tray::TrayAction::ShowSettings => {
                    if let Some(w) = window_weak.upgrade() {
                        if let Ok(cfg) = config_for_tray.read() {
                            update_gui_from_config(&w, &cfg);
                        }
                        w.show().ok();
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
                    log::info!("Monitoring {}", if was_paused { "resumed" } else { "paused" });
                }
                tray::TrayAction::Quit => {
                    log::info!("Quit requested from tray");
                    slint::quit_event_loop().ok();
                }
                tray::TrayAction::None => {}
            }
        },
    );

    // Refresh active windows in the GUI from the monitor's snapshot buffer
    let snapshot_for_gui = snapshot_buffer.clone();
    let config_for_gui = config.clone();
    let visible_for_gui = window_visible.clone();
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
                }
            }
        },
    );

    // Spawn monitor thread
    let monitor_config = config.clone();
    let monitor_paused = paused.clone();
    let monitor_stop = should_stop.clone();
    let monitor_snapshot = snapshot_buffer.clone();
    let monitor_thread = std::thread::spawn(move || {
        let api = Win32Api::new();
        let mut monitor = Monitor::new(api, monitor_config, monitor_paused, monitor_snapshot);
        monitor.run(monitor_stop);
    });

    // Run Slint event loop (blocks until quit)
    // Don't quit when last window is hidden
    let _ = slint::run_event_loop_until_quit();

    // Cleanup
    log::info!("Shutting down...");
    should_stop.store(true, Ordering::Relaxed);
    if let Err(_) = monitor_thread.join() {
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

/// Populate Slint GUI properties from the Config struct.
fn update_gui_from_config(window: &SettingsWindow, config: &Config) {
    // App rules
    let rules: Vec<AppRuleModel> = config
        .app_rule
        .iter()
        .map(|r| AppRuleModel {
            process: r.process.clone().into(),
            timeout_mins: r.timeout_mins as i32,
            action: r.action.as_str().into(),
            enabled: r.enabled,
        })
        .collect();
    window.set_app_rules(std::rc::Rc::new(slint::VecModel::from(rules)).into());

    // Buckets
    let buckets: Vec<BucketModel> = config
        .bucket
        .iter()
        .map(|b| BucketModel {
            name: b.name.clone().into(),
            timeout_mins: b.timeout_mins as i32,
            action: b.action.as_str().into(),
            enabled: b.enabled,
            processes: b.processes.join(", ").into(),
        })
        .collect();
    window.set_buckets(std::rc::Rc::new(slint::VecModel::from(buckets)).into());

    // General
    window.set_polling_interval_secs(config.general.polling_interval_secs as i32);
    window.set_auto_start(config.general.auto_start);
}

/// Refresh the active windows model in the GUI using current config + snapshot buffer.
fn refresh_active_windows(
    window: &SettingsWindow,
    config: &Config,
    snapshot_buffer: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
) {
    if let Ok(buf) = snapshot_buffer.lock() {
        let models: Vec<ActiveWindowModel> = buf
            .iter()
            .filter(|s| !config.is_hidden(&s.process))
            .map(|s| ActiveWindowModel {
                process: s.process.clone().into(),
                title: s.title.clone().into(),
                idle_secs: s.idle_secs as i32,
                managed: config.resolve_process(&s.process.to_lowercase()).is_some(),
            })
            .collect();
        window.set_active_windows(
            std::rc::Rc::new(slint::VecModel::from(models)).into(),
        );
    }
}

/// Wire Slint callbacks to modify the shared config.
fn setup_gui_callbacks(
    window: &SettingsWindow,
    config: Arc<RwLock<Config>>,
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
) {
    // Helper: refresh rules + active windows after any rule/bucket mutation
    let refresh_all = move |cfg: &Config,
                            weak: &slint::Weak<SettingsWindow>,
                            snap: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>| {
        if let Some(w) = weak.upgrade() {
            // Refresh rules
            let rules: Vec<AppRuleModel> = cfg
                .app_rule
                .iter()
                .map(|r| AppRuleModel {
                    process: r.process.clone().into(),
                    timeout_mins: r.timeout_mins as i32,
                    action: r.action.as_str().into(),
                    enabled: r.enabled,
                })
                .collect();
            w.set_app_rules(std::rc::Rc::new(slint::VecModel::from(rules)).into());

            // Refresh active windows (updates managed status, removes hidden)
            refresh_active_windows(&w, cfg, snap);
        }
    };

    // Add rule
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_add_rule(move |process| {
        let process_str = process.to_string();
        if process_str.is_empty() {
            return;
        }
        if let Ok(mut c) = cfg.write() {
            if c.app_rule.iter().any(|r| r.process.eq_ignore_ascii_case(&process_str)) {
                return;
            }
            c.app_rule.push(config::AppRule {
                process: process_str,
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
            });
            let _ = c.save();
            refresh_all(&c, &weak, &snap);
        }
    });

    // Remove rule
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_remove_rule(move |idx| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.app_rule.len() {
                c.app_rule.remove(idx);
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    // Toggle rule enabled
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_toggle_rule(move |idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.app_rule.len() {
                c.app_rule[idx].enabled = enabled;
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    // Update rule timeout
    let cfg = config.clone();
    window.on_update_rule_timeout(move |idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.app_rule.len() {
                c.app_rule[idx].timeout_mins = mins as u64;
                let _ = c.save();
            }
        }
    });

    // Update rule action
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_update_rule_action(move |idx, action| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.app_rule.len() {
                c.app_rule[idx].action = Action::from_str(&action);
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    // Hide process
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_hide_process(move |process| {
        if let Ok(mut c) = cfg.write() {
            let process_str = process.to_string();
            if !c.general.hidden_processes.contains(&process_str) {
                c.general.hidden_processes.push(process_str);
                let _ = c.save();
                refresh_active_windows(&weak.upgrade().unwrap(), &c, &snap);
            }
        }
    });

    // Toggle bucket
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_toggle_bucket(move |idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].enabled = enabled;
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    // Update bucket action
    let cfg = config.clone();
    window.on_update_bucket_action(move |idx, action| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].action = Action::from_str(&action);
                let _ = c.save();
            }
        }
    });

    // Update bucket timeout
    let cfg = config.clone();
    window.on_update_bucket_timeout(move |idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].timeout_mins = mins as u64;
                let _ = c.save();
            }
        }
    });

    // Set polling interval
    let cfg = config.clone();
    window.on_set_polling_interval(move |secs| {
        if let Ok(mut c) = cfg.write() {
            c.general.polling_interval_secs = secs as u64;
            let _ = c.save();
        }
    });

    // Set auto-start
    let cfg = config.clone();
    window.on_set_auto_start(move |enabled| {
        if let Err(e) = autostart::set_auto_start(enabled) {
            log::error!("Auto-start toggle failed: {}", e);
            return;
        }
        if let Ok(mut c) = cfg.write() {
            c.general.auto_start = enabled;
            let _ = c.save();
        }
    });
}

/// Fallback: run without GUI if Slint window creation fails.
fn run_headless(config: Arc<RwLock<Config>>, paused: Arc<AtomicBool>, should_stop: Arc<AtomicBool>) {
    log::warn!("Running in headless mode (no GUI)");
    let api = Win32Api::new();
    let dummy_buffer = Arc::new(Mutex::new(Vec::new()));
    let mut monitor = Monitor::new(api, config, paused, dummy_buffer);
    monitor.run(should_stop);
}
