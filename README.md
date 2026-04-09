# Fade

Automatically minimizes or closes windows that have been idle too long. Set a timeout per app or per category, and Fade quietly cleans up your desktop while you work. Inspired by [Quitter](https://marco.org/apps) by Marco Arment.

## Features

- **Per-app rules** — set a timeout and action (minimize or close) for any process
- **Buckets** — predefined groups (Browsing, Communication, Media, Development, Gaming) that manage multiple apps with one setting
- **System tray** — runs silently in the background; double-click the tray icon or right-click for settings, pause, or quit
- **Settings GUI** — manage rules, view active windows sorted by idle time, configure buckets and general settings
- **Auto-start** — optionally start with Windows
- **Portable** — config file lives next to the executable, no installer required
- **Instant transitions** — windows disappear immediately, no slow animations

## Installation

Download the latest release, or build from source:

```bash
cargo build --release
```

The binary is at `target/release/fade.exe`. Copy it wherever you like — Fade stores its config (`fade.toml`) next to the executable.

## Usage

Run `fade.exe`. It starts minimized to the system tray.

- **Double-click** the tray icon to open Settings
- **Right-click** the tray icon for a menu: Settings, Pause/Resume, Quit

### Rules Tab

Add processes (e.g., `chrome.exe`) with a timeout in minutes and an action (minimize or close). When a managed window has been in the background longer than its timeout, Fade acts on it.

### Buckets Tab

Enable a predefined group to manage all its apps at once. For example, enabling "Browsing" manages Chrome, Firefox, Edge, Brave, Opera, Vivaldi, and Arc with a single timeout.

App-specific rules take priority over bucket membership.

### Active Windows Tab

Shows all currently open windows sorted by idle time. Useful for seeing what Fade is tracking.

### General Tab

- **Polling interval** — how often Fade checks for idle windows (15, 30, or 60 seconds)
- **Start with Windows** — register Fade to launch at login

## Configuration

Fade stores its settings in `fade.toml` next to the executable. You can edit it directly:

```toml
[general]
polling_interval_secs = 30
auto_start = false
hidden_processes = ["explorer.exe", "SearchHost.exe"]

[[bucket]]
name = "Browsing"
processes = ["chrome.exe", "firefox.exe", "msedge.exe"]
timeout_mins = 15
action = "minimize"
enabled = true

[[app_rule]]
process = "slack.exe"
timeout_mins = 30
action = "minimize"
enabled = true
```

## Building from Source

Requirements:
- Rust toolchain (stable)
- Windows SDK (for full functionality; builds on Linux/macOS with stubs for development)

```bash
cargo build --release    # optimized build (~small binary with LTO + strip)
cargo test               # run tests (works on any platform)
```

## How It Works

Fade polls the system every N seconds. For each visible window, it checks how long ago that window's process was last in the foreground. If the idle time exceeds the configured timeout and the window isn't fullscreen, Fade minimizes or closes it. The foreground window is never touched.

System windows (taskbar, desktop, DWM, etc.) are automatically filtered out.
