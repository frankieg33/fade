# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

**Fade** is a Windows system tray application that automatically minimizes or closes inactive windows after configurable idle timeouts. It runs as a background process with a Slint-based settings GUI accessible from the tray.

## Commands

```bash
# Build Windows binary from WSL (debug)
cargo build --target x86_64-pc-windows-gnu

# Build Windows binary from WSL (release — opt-level=z, LTO, strip)
cargo build --release --target x86_64-pc-windows-gnu

# Run all tests (Linux-native — tests compile and run via the stub WindowApi)
cargo test

# Run a single test
cargo test test_idle_process_gets_minimized

# Run tests in a specific module
cargo test --test '*' -- monitor::tests

# Check without building
cargo check

# Run with logging (Windows only — requires running the .exe on Windows)
RUST_LOG=debug cargo run
```

### WSL Prerequisites

```bash
sudo apt install mingw-w64 pkg-config libfontconfig1-dev
rustup target add x86_64-pc-windows-gnu
```

## Architecture

### Core Data Flow

1. **`config.rs`** — `Config` (TOML) is loaded at startup and wrapped in `Arc<RwLock<Config>>` shared between the monitor thread and GUI callbacks. Every GUI mutation calls `config.save()` immediately (atomic write via temp-file rename).

2. **`monitor.rs`** — `Monitor<W: WindowApi>` runs on a dedicated thread. Each poll cycle: gets the foreground process, enumerates visible windows, checks idle time per process against config rules, then minimizes/closes overdue windows. The `WindowApi` trait is the seam for testability.

3. **`winapi.rs`** — `Win32Api` implements `WindowApi` with real Win32 calls (all `unsafe` isolated here). A `MockWindowApi` (in `#[cfg(test)]`) enables unit tests to run cross-platform without Win32.

4. **`main.rs`** — Wires everything together: creates the Slint window, spawns the monitor thread, polls tray events via a 100ms Slint timer, refreshes the Active Windows tab via a 2s Slint timer reading from a shared snapshot buffer, and runs the Slint event loop. GUI callbacks modify the shared `Arc<RwLock<Config>>` directly.

5. **`ui/main.slint`** — Slint UI compiled at build time via `build.rs`. Defines `SettingsWindow` with three data models (`AppRuleModel`, `BucketModel`, `ActiveWindowModel`) and callbacks that map to Rust closures in `setup_gui_callbacks`.

### Cross-Thread Communication

- `Arc<RwLock<Config>>` — shared config, written by GUI callbacks, read by monitor
- `Arc<AtomicBool>` — `paused` and `should_stop` flags
- `Arc<Mutex<Vec<ActiveWindowSnapshot>>>` — monitor publishes window snapshots after each poll, GUI refresh timer reads them when the settings window is visible

### Rule Resolution Priority

`app_rule` entries take priority over `bucket` membership. Both are case-insensitive on process name. Buckets (Browsing, Communication, Media, etc.) are opt-in (disabled by default).

### Cross-Platform Compilation

The codebase compiles on Linux/macOS for testing. Win32-specific code is gated with `#[cfg(target_os = "windows")]`. The `Win32Api` stub on non-Windows returns empty/no-op results. `tray-icon`, `muda`, and `image` crates are Windows-only dependencies.

### Key Design Constraints

- All Win32 `unsafe` calls are confined to `winapi.rs` — keep it that way.
- The tray icon must be created on the same thread as the Slint event loop (Win32 message pump requirement).
- `Config::save()` uses atomic rename (write to `.toml.tmp`, then rename) to avoid partial writes.
- `filter.rs` hardcodes system process/class blocklists — `windowsterminal.exe` is intentionally filtered (comment explains this is debatable).
