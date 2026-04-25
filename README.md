# Fade

Automatically minimizes or closes windows that have been idle too long. Set a timeout per app or per category, and Fade quietly cleans up your desktop while you work. Inspired by [Quitter](https://marco.org/apps) by Marco Arment.

## Features

- **Per-app rules** — timeout and action (minimize or close) for any process
- **Bucket groups** — predefined groups (Browsing, Communication, Media, Development, Gaming, Utilities) that manage multiple apps with one setting; app-specific rules override bucket rules
- **System tray** — runs silently in the background; right-click for settings, pause, or quit
- **Brand-accurate icons** — recognizes most common apps (Chrome, Firefox, Slack, VSCode, Steam, etc.) and falls back to themed glyphs for the rest
- **Activity tab** — see what's currently being tracked and recent actions Fade has taken
- **Auto-start** — optional launch with Windows
- **Portable** — config file lives next to the executable, no installer required

## Install

1. Download the latest `fade-vX.Y.Z-windows-x86_64.zip` from [Releases](../../releases).
2. Unzip somewhere stable — Fade stores its config (`fade.toml`) next to the executable.
3. Run `fade.exe`. The settings window opens once on first launch; afterwards Fade lives in the system tray.

## Using it

- **Right-click the tray icon** for show settings / pause / quit.
- **Closing the settings window hides to the tray** — it doesn't quit. Use tray → Quit to exit.

### Rules tab

Edit per-app and per-bucket rules. Each row has a timeout slider, a minimize/close action, and an enable toggle. Buckets can be expanded to manage their member apps individually. Add new processes with the inline `Add process name` field.

### Activity tab

Shows currently-running tracked apps (with idle time) and a rolling log of the last 100 actions Fade has taken.

### Settings tab

Polling interval, start-with-Windows toggle, window-position memory.

## Configuration

`fade.toml` lives next to the executable. Out-of-range values get clamped on load (polling interval `[1, 3600]` s, timeouts `[1, 10080]` minutes / 1 week).

If the config file is corrupted, it's renamed to `fade.toml.corrupt-<timestamp>` and Fade falls back to defaults — your hand-edits aren't lost.

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

## Build from source

Cross-compile from Linux/WSL via the MinGW toolchain:

```bash
sudo apt install mingw-w64 pkg-config libfontconfig1-dev
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

You can also build natively on Windows with the MSVC toolchain (`cargo build --release`).

Tests run on any platform — Win32 calls are isolated behind a `WindowApi` trait with a mock for cross-platform testing:

```bash
cargo test
```

## How It Works

Fade polls the system every N seconds (default 30). For each visible window, it checks how long since the owning process was last in the foreground. If the idle time exceeds the configured timeout and the window isn't fullscreen or filtered, Fade minimizes or closes it. The current foreground window is never touched. System windows (taskbar, desktop, DWM, etc.) are filtered out automatically.

A Win32 `SetWinEventHook` keeps foreground timestamps current in real time, so newly-active windows reset their timers immediately rather than waiting for the next poll.

## License

MIT — see [LICENSE](LICENSE).

Brand icons under [assets/icons/](assets/icons/) are bundled from [simpleicons.org](https://simpleicons.org/) (CC0) and [Tabler Icons](https://github.com/tabler/tabler-icons) (MIT). See [assets/icons/LICENSES.md](assets/icons/LICENSES.md) for full attribution.
