use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Minimize,
    Close,
}

impl Default for Action {
    fn default() -> Self {
        Action::Minimize
    }
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
            _ => Action::Minimize,
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
}

impl Default for General {
    fn default() -> Self {
        Self {
            polling_interval_secs: 30,
            auto_start: false,
            hidden_processes: vec![
                "explorer.exe".into(),
                "SearchHost.exe".into(),
                "StartMenuExperienceHost.exe".into(),
                "ShellExperienceHost.exe".into(),
                "TextInputHost.exe".into(),
            ],
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
}

fn default_timeout() -> u64 {
    15
}

fn default_true() -> bool {
    true
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
    /// Falls back to defaults if file is missing or corrupt.
    pub fn load() -> Self {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str::<Config>(&contents) {
                Ok(config) => {
                    log::info!("Loaded config from {}", path.display());
                    config
                }
                Err(e) => {
                    log::warn!("Config parse error ({}), using defaults: {}", path.display(), e);
                    Self::default_config()
                }
            },
            Err(_) => {
                log::info!("No config file found, creating defaults at {}", path.display());
                let config = Self::default_config();
                let _ = config.save(); // best-effort save
                config
            }
        }
    }

    /// Save config to the standard path.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        let contents = toml::to_string_pretty(self).map_err(|e| format!("Serialize error: {}", e))?;

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
            }],
            app_rule: vec![AppRule {
                process: "chrome.exe".into(),
                timeout_mins: 5,
                action: Action::Close,
                enabled: true,
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
            }],
        };

        assert!(config.resolve_process("chrome.exe").is_some());
        assert!(config.resolve_process("CHROME.EXE").is_some());
    }

    #[test]
    fn test_is_hidden() {
        let config = Config::default_config();
        assert!(config.is_hidden("explorer.exe"));
        assert!(config.is_hidden("Explorer.EXE"));
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
        assert_eq!(Action::from_str(Action::Minimize.as_str()), Action::Minimize);
        assert_eq!(Action::from_str(Action::Close.as_str()), Action::Close);
        assert_eq!(Action::from_str("garbage"), Action::Minimize); // default fallback
    }
}
