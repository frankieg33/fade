use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Action {
    #[default]
    Minimize,
    Close,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Minimize => "minimize",
            Action::Close => "close",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "close" => Action::Close,
            "minimize" => Action::Minimize,
            other => {
                log::warn!("Unknown action '{}', defaulting to minimize", other);
                Action::Minimize
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct General {
    #[serde(default = "default_poll")]
    pub polling_interval_secs: u64,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub hidden_processes: Vec<String>,
    /// Window geometry persisted across sessions. None = use preferred.
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
}

impl Default for General {
    fn default() -> Self {
        Self {
            polling_interval_secs: 30,
            auto_start: false,
            hidden_processes: vec![
                "SearchHost.exe".into(),
                "StartMenuExperienceHost.exe".into(),
                "ShellExperienceHost.exe".into(),
                "TextInputHost.exe".into(),
            ],
            window_width: None,
            window_height: None,
            window_x: None,
            window_y: None,
        }
    }
}

fn default_poll() -> u64 {
    30
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bucket {
    pub name: String,
    pub processes: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_mins: u64,
    #[serde(default)]
    pub action: Action,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub expanded: bool,
    #[serde(default)]
    pub icon: Option<String>,
}

fn default_timeout() -> u64 {
    15
}

fn default_true() -> bool {
    true
}

/// Is this app "customized" in the UI sense (inline slider/combo visible)?
/// True if either (a) the user explicitly clicked Edit, or (b) the rule's
/// timeout/action diverges from the group's current values.
pub fn app_is_customized(bucket: &Bucket, rule: &AppRule) -> bool {
    rule.customized || rule.timeout_mins != bucket.timeout_mins || rule.action != bucket.action
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppRule {
    pub process: String,
    #[serde(default = "default_timeout")]
    pub timeout_mins: u64,
    #[serde(default)]
    pub action: Action,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub icon: Option<String>,
    /// True when the user explicitly clicked "Edit" to customize this app.
    /// Icon-only overrides (via picker) leave this false so the inline
    /// slider/combo don't appear for apps that just have a custom icon.
    #[serde(default)]
    pub customized: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub bucket: Vec<Bucket>,
    #[serde(default)]
    pub app_rule: Vec<AppRule>,
}

/// Result of resolving what action to take for a given process.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRule {
    pub timeout_mins: u64,
    pub action: Action,
}

impl Config {
    /// Look up the effective rule for a process name.
    /// App rules take priority over bucket membership.
    /// Returns None if the process is not managed.
    pub fn resolve_process(&self, process: &str) -> Option<ResolvedRule> {
        let process_lower = process.to_lowercase();

        // Check app_rules first (highest priority)
        for rule in &self.app_rule {
            if rule.enabled && rule.process.to_lowercase() == process_lower {
                return Some(ResolvedRule {
                    timeout_mins: rule.timeout_mins,
                    action: rule.action.clone(),
                });
            }
        }

        // Check buckets
        for bucket in &self.bucket {
            if !bucket.enabled {
                continue;
            }
            for proc in &bucket.processes {
                if proc.to_lowercase() == process_lower {
                    return Some(ResolvedRule {
                        timeout_mins: bucket.timeout_mins,
                        action: bucket.action.clone(),
                    });
                }
            }
        }

        None
    }

    /// Resolve the icon glyph to display for a process, preferring an override on its AppRule.
    pub fn icon_for_app(&self, process: &str) -> String {
        let process_lower = process.to_lowercase();
        self.app_rule
            .iter()
            .find(|r| r.process.to_lowercase() == process_lower)
            .and_then(|r| r.icon.clone())
            .unwrap_or_else(|| crate::icons::process_icon(process).to_string())
    }

    /// Resolve the icon glyph to display for a bucket by index.
    pub fn icon_for_bucket(&self, idx: usize) -> String {
        self.bucket
            .get(idx)
            .map(|b| {
                b.icon
                    .clone()
                    .unwrap_or_else(|| crate::icons::bucket_icon(&b.name).to_string())
            })
            .unwrap_or_default()
    }

    /// Check if a process is in the hidden list.
    #[allow(dead_code)]
    pub fn is_hidden(&self, process: &str) -> bool {
        let process_lower = process.to_lowercase();
        self.general
            .hidden_processes
            .iter()
            .any(|p| p.to_lowercase() == process_lower)
    }

    /// Build default config with predefined buckets.
    pub fn default_config() -> Self {
        Config {
            general: General::default(),
            bucket: default_buckets(),
            app_rule: Vec::new(),
        }
    }

    /// Load config from the standard path (next to exe).
    /// Falls back to defaults if file is missing or corrupt. A corrupt config
    /// is preserved as `<path>.corrupt-<unix_ts>` so the user can recover.
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(mut config) => {
                    config.clamp_ranges();
                    log::info!("Loaded config from {}", path.display());
                    config
                }
                Err(e) => {
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let backup = path.with_extension(format!("toml.corrupt-{}", ts));
                    let backup_msg = match std::fs::rename(&path, &backup) {
                        Ok(()) => format!("preserved as {}", backup.display()),
                        Err(re) => format!("could not back up ({})", re),
                    };
                    log::error!(
                        "Config parse error at {}; reverting to defaults — {}: {}",
                        path.display(),
                        backup_msg,
                        e
                    );
                    Self::default_config()
                }
            },
            Err(_) => {
                log::info!(
                    "No config file found, creating defaults at {}",
                    path.display()
                );
                let config = Self::default_config();
                if let Err(e) = config.save() {
                    log::error!("failed to write initial config: {}", e);
                }
                config
            }
        }
    }

    /// Clamp out-of-range values that may have been hand-edited or saved by an
    /// older buggy build. Called after every successful deserialize.
    fn clamp_ranges(&mut self) {
        // Polling interval: at least 1s (lower would tight-loop the monitor),
        // at most 1 hour (anything bigger means the user effectively disabled it).
        self.general.polling_interval_secs = self.general.polling_interval_secs.clamp(1, 3600);
        // Timeouts: 1 min to 7 days. Below 1 windows would minimize before the
        // user blinked; above 7 days the rule is effectively disabled.
        const MAX_TIMEOUT: u64 = 7 * 24 * 60;
        for rule in &mut self.app_rule {
            rule.timeout_mins = rule.timeout_mins.clamp(1, MAX_TIMEOUT);
        }
        for bucket in &mut self.bucket {
            bucket.timeout_mins = bucket.timeout_mins.clamp(1, MAX_TIMEOUT);
        }
    }

    /// Save config to the standard path.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        let contents =
            toml::to_string_pretty(self).map_err(|e| format!("Serialize error: {}", e))?;

        // Write to temp file, then rename for atomic save
        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &contents).map_err(|e| format!("Write error: {}", e))?;
        std::fs::rename(&tmp_path, &path).map_err(|e| {
            // Clean up temp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            format!("Rename error: {}", e)
        })?;

        log::info!("Saved config to {}", path.display());
        Ok(())
    }
}

/// Config file path: next to the executable (portable mode).
pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("fade.exe"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("fade.toml")
}

/// Predefined bucket definitions.
fn default_buckets() -> Vec<Bucket> {
    vec![
        Bucket {
            name: "Browsing".into(),
            processes: vec![
                "chrome.exe".into(),
                "firefox.exe".into(),
                "msedge.exe".into(),
                "brave.exe".into(),
                "opera.exe".into(),
                "vivaldi.exe".into(),
                "Arc.exe".into(),
            ],
            timeout_mins: 15,
            action: Action::Minimize,
            enabled: false, // opt-in
            expanded: true,
            icon: None,
        },
        Bucket {
            name: "Communication".into(),
            processes: vec![
                "slack.exe".into(),
                "Discord.exe".into(),
                "teams.exe".into(),
                "Telegram.exe".into(),
                "Signal.exe".into(),
                "WhatsApp.exe".into(),
            ],
            timeout_mins: 30,
            action: Action::Minimize,
            enabled: false,
            expanded: true,
            icon: None,
        },
        Bucket {
            name: "Media".into(),
            processes: vec![
                "Spotify.exe".into(),
                "vlc.exe".into(),
                "iTunes.exe".into(),
                "foobar2000.exe".into(),
            ],
            timeout_mins: 20,
            action: Action::Minimize,
            enabled: false,
            expanded: true,
            icon: None,
        },
        Bucket {
            name: "Development".into(),
            processes: vec![
                "Code.exe".into(),
                "idea64.exe".into(),
                "studio64.exe".into(),
                "devenv.exe".into(),
            ],
            timeout_mins: 60,
            action: Action::Minimize,
            enabled: false,
            expanded: true,
            icon: None,
        },
        Bucket {
            name: "Gaming".into(),
            processes: vec![
                "steam.exe".into(),
                "EpicGamesLauncher.exe".into(),
                "GalaxyClient.exe".into(),
            ],
            timeout_mins: 30,
            action: Action::Minimize,
            enabled: false,
            expanded: true,
            icon: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_has_buckets() {
        let config = Config::default_config();
        assert!(config.bucket.len() >= 5);
        assert!(config.app_rule.is_empty());
    }

    #[test]
    fn test_resolve_app_rule_priority() {
        let config = Config {
            general: General::default(),
            bucket: vec![Bucket {
                name: "Browsing".into(),
                processes: vec!["chrome.exe".into()],
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
                expanded: true,
                icon: None,
            }],
            app_rule: vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 5,
                action: Action::Close,
                enabled: true,
                icon: None,
                customized: false,
            }],
        };

        let resolved = config.resolve_process("chrome.exe").unwrap();
        assert_eq!(resolved.timeout_mins, 5);
        assert_eq!(resolved.action, Action::Close);
    }

    #[test]
    fn test_resolve_bucket_fallback() {
        let config = Config {
            general: General::default(),
            bucket: vec![Bucket {
                name: "Browsing".into(),
                processes: vec!["chrome.exe".into()],
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: true,
                expanded: true,
                icon: None,
            }],
            app_rule: vec![],
        };

        let resolved = config.resolve_process("chrome.exe").unwrap();
        assert_eq!(resolved.timeout_mins, 15);
        assert_eq!(resolved.action, Action::Minimize);
    }

    #[test]
    fn test_resolve_unmanaged_returns_none() {
        let config = Config::default_config();
        assert!(config.resolve_process("unknown.exe").is_none());
    }

    #[test]
    fn test_resolve_disabled_rule_skipped() {
        let config = Config {
            general: General::default(),
            bucket: vec![],
            app_rule: vec![AppRule {
                process: "notepad.exe".into(),
                timeout_mins: 5,
                action: Action::Close,
                enabled: false,
                icon: None,
                customized: false,
            }],
        };

        assert!(config.resolve_process("notepad.exe").is_none());
    }

    #[test]
    fn test_resolve_disabled_bucket_skipped() {
        let config = Config {
            general: General::default(),
            bucket: vec![Bucket {
                name: "Test".into(),
                processes: vec!["chrome.exe".into()],
                timeout_mins: 15,
                action: Action::Minimize,
                enabled: false,
                expanded: true,
                icon: None,
            }],
            app_rule: vec![],
        };

        assert!(config.resolve_process("chrome.exe").is_none());
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let config = Config {
            general: General::default(),
            bucket: vec![],
            app_rule: vec![AppRule {
                process: "Chrome.exe".into(),
                timeout_mins: 10,
                action: Action::Minimize,
                enabled: true,
                icon: None,
                customized: false,
            }],
        };

        assert!(config.resolve_process("chrome.exe").is_some());
        assert!(config.resolve_process("CHROME.EXE").is_some());
    }

    #[test]
    fn test_is_hidden() {
        let config = Config::default_config();
        assert!(config.is_hidden("SearchHost.exe"));
        assert!(config.is_hidden("SEARCHHOST.EXE"));
        assert!(!config.is_hidden("chrome.exe"));
    }

    #[test]
    fn test_parse_valid_toml() {
        let toml_str = r#"
[general]
polling_interval_secs = 15
auto_start = true
hidden_processes = ["explorer.exe"]

[[bucket]]
name = "Test"
processes = ["test.exe"]
timeout_mins = 10
action = "close"
enabled = true

[[app_rule]]
process = "notepad.exe"
timeout_mins = 5
action = "minimize"
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.polling_interval_secs, 15);
        assert!(config.general.auto_start);
        assert_eq!(config.bucket.len(), 1);
        assert_eq!(config.bucket[0].action, Action::Close);
        assert_eq!(config.app_rule.len(), 1);
    }

    #[test]
    fn test_parse_empty_toml_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.general.polling_interval_secs, 30);
        assert!(!config.general.auto_start);
        assert!(config.bucket.is_empty());
    }

    #[test]
    fn test_parse_corrupt_toml_handled() {
        let result = toml::from_str::<Config>("{{{{not valid toml");
        assert!(result.is_err());
        // Config::load() would fall back to defaults here
    }

    #[test]
    fn test_roundtrip_serialize() {
        let config = Config::default_config();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(config.bucket.len(), deserialized.bucket.len());
        assert_eq!(
            config.general.polling_interval_secs,
            deserialized.general.polling_interval_secs
        );
    }

    #[test]
    fn test_action_str_roundtrip() {
        assert_eq!(
            Action::from_str(Action::Minimize.as_str()),
            Action::Minimize
        );
        assert_eq!(Action::from_str(Action::Close.as_str()), Action::Close);
        assert_eq!(Action::from_str("garbage"), Action::Minimize); // default fallback
    }

    #[test]
    fn test_expanded_defaults_to_true_from_old_toml() {
        let toml_str = r#"
[[bucket]]
name = "Test"
processes = ["test.exe"]
timeout_mins = 10
action = "minimize"
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.bucket.len(), 1);
        assert!(
            config.bucket[0].expanded,
            "expanded should default to true for old configs"
        );
    }

    #[test]
    fn test_icon_defaults_to_none_from_old_toml() {
        let toml_str = r#"
[[bucket]]
name = "Test"
processes = ["test.exe"]
timeout_mins = 10
action = "minimize"
enabled = true

[[app_rule]]
process = "foo.exe"
timeout_mins = 5
action = "close"
enabled = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.bucket[0].icon.is_none());
        assert!(config.app_rule[0].icon.is_none());
    }

    #[test]
    fn test_icon_roundtrip() {
        let mut config = Config::default_config();
        config.bucket[0].icon = Some("\u{F0E52}".into());
        config.app_rule.push(AppRule {
            process: "foo.exe".into(),
            timeout_mins: 10,
            action: Action::Minimize,
            enabled: true,
            icon: Some("\u{F0483}".into()),
            customized: false,
        });
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.bucket[0].icon.as_deref(), Some("\u{F0E52}"));
        assert_eq!(deserialized.app_rule[0].icon.as_deref(), Some("\u{F0483}"));
    }

    #[test]
    fn test_app_is_customized() {
        let bucket = Bucket {
            name: "B".into(),
            processes: vec!["p.exe".into()],
            timeout_mins: 15,
            action: Action::Minimize,
            enabled: true,
            expanded: true,
            icon: None,
        };
        let base_rule = AppRule {
            process: "p.exe".into(),
            timeout_mins: 15,
            action: Action::Minimize,
            enabled: true,
            icon: None,
            customized: false,
        };
        assert!(!app_is_customized(&bucket, &base_rule));
        // Icon-only diff: NOT customized
        let mut r = base_rule.clone();
        r.icon = Some("x".into());
        assert!(!app_is_customized(&bucket, &r));
        // Timeout diff: customized
        let mut r = base_rule.clone();
        r.timeout_mins = 5;
        assert!(app_is_customized(&bucket, &r));
        // Action diff: customized
        let mut r = base_rule.clone();
        r.action = Action::Close;
        assert!(app_is_customized(&bucket, &r));
    }

    #[test]
    fn test_icon_for_app_fallback_and_override() {
        let mut config = Config::default_config();
        // No rule → falls back to catalog
        let fallback = config.icon_for_app("chrome.exe");
        assert_eq!(fallback, "googlechrome"); // chrome maps to brand slug now
                                              // With rule override → uses override
        config.app_rule.push(AppRule {
            process: "chrome.exe".into(),
            timeout_mins: 15,
            action: Action::Minimize,
            enabled: true,
            icon: Some("XYZ".into()),
            customized: false,
        });
        assert_eq!(config.icon_for_app("chrome.exe"), "XYZ");
    }

    #[test]
    fn test_icon_for_bucket_fallback_and_override() {
        let mut config = Config::default_config();
        let first = config.icon_for_bucket(0);
        assert!(!first.is_empty());
        config.bucket[0].icon = Some("OVERRIDE".into());
        assert_eq!(config.icon_for_bucket(0), "OVERRIDE");
    }

    #[test]
    fn test_expanded_roundtrip() {
        let mut config = Config::default_config();
        config.bucket[0].expanded = false;
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert!(
            !deserialized.bucket[0].expanded,
            "expanded=false should survive roundtrip"
        );
        assert!(
            deserialized.bucket[1].expanded,
            "other buckets should stay expanded"
        );
    }
}
