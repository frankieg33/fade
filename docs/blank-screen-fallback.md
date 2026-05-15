# Blank-screen / stale-framebuffer fallback playbook

If a user reports that the Fade settings window goes blank, paints partially,
shows the desktop wallpaper through the window, or "stops refreshing" after
sitting idle — and the current `redraw-tick` mechanism (full-bleed backdrop
opacity bump in `ui/main.slint`, driven by `force_repaint` in `src/main.rs`)
is **already in place** — this is the escalation order. Each step is a real
code change, not a config tweak.

## Background — what's already done

Three layers of defense already exist. Confirm all three before escalating.

1. **2 s gui-refresh timer** (`src/main.rs`, `gui_refresh_timer`) calls
   `force_repaint(&w)` every tick while the window is visible.
2. **`force_repaint`** bumps `redraw_tick` (modulo 1000) and calls
   `window().request_redraw()`.
3. **Full-bleed backdrop in `ui/main.slint`** binds its `opacity` to
   `redraw-tick`, so the property change marks the entire window dirty in
   Slint's damage tracker. (An earlier 1×1 px sentinel was insufficient —
   it only invalidated a 1px tile and let the rest of the framebuffer go
   stale. See commit `8d82451` for the rationale.)

If a regression report comes in and any of these is missing, restore them
before doing anything new.

## Step 1 — Add Win32 `RedrawWindow` belt-and-suspenders

The damage tracker fix dirties Slint's scene. This step also forces the OS
to deliver a fresh `WM_PAINT` to the HWND, defending against scenarios where
Slint's event pump is throttled (window occluded, monitor sleep, OS thinks
the window doesn't need painting).

In `src/winapi.rs`, add a thin helper:

```rust
#[cfg(target_os = "windows")]
pub fn invalidate_hwnd(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Graphics::Gdi::{RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE};
    unsafe {
        let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
    }
}
```

Add `Win32_Graphics_Gdi` to the `windows` crate feature list in `Cargo.toml`
if `RedrawWindow` is not already pulled in (it currently is — verify).

In `src/main.rs::force_repaint`, after `request_redraw()`, get the HWND from
the Slint window via `raw-window-handle` 0.6 and call `invalidate_hwnd`. The
Slint API for this is `w.window().window_handle()` returning a
`Result<WindowHandle, _>`. `WindowHandle` is a wrapper around the
`RawWindowHandle` enum, so match on `handle.as_raw()` and pull out
`RawWindowHandle::Win32(h)`, then pass `HWND(h.hwnd.get() as _)`.

Gate the HWND extraction with `#[cfg(target_os = "windows")]` so the Linux
test build keeps working.

## Step 2 — Switch from `renderer-software` to `renderer-femtovg`

Only do this if Step 1 still doesn't fix it. The Slint software renderer
has known damage-tracking pathologies on long-lived windows; FemtoVG is
GPU-accelerated and doesn't share them.

In `Cargo.toml`, replace the `slint` dependency:

```toml
slint = { version = "1.11", default-features = false, features = [
    "std", "compat-1-2", "backend-winit", "renderer-femtovg", "image-default-formats"
] }
```

Then remove the `redraw-tick` plumbing (the backdrop opacity binding, the
property declaration, the `force_repaint` helper, and the timer call site)
because it stops being load-bearing — but leave the gui-refresh timer
itself; it still drives data refresh.

Trade-offs to call out in the PR description:
- Adds an OpenGL runtime requirement (fine on modern Windows, can break on
  older VMs / RDP / broken GPU drivers — keep the headless fallback in
  `run_headless`).
- Release binary grows by a few MB.
- If FemtoVG refuses to initialize, `SettingsWindow::new()` returns `Err`
  and Fade falls back to `run_headless` — that's the existing safety net.

## Step 3 — Last resort: window recreation

If both above fail, the next move is to recreate the Slint window (drop
the old `SettingsWindow`, build a new one with the same geometry) on the
2 s timer. This is jarring and should not be the answer; if you find
yourself here, the right move is probably to file a Slint upstream issue
with a minimal repro instead.

## What the symptom actually means

- **Desktop wallpaper visible through the window** + a few UI elements
  painted → Slint scene is rendering, but the framebuffer that gets
  blitted is stale. Pure renderer/damage-tracker problem. Steps 1–3 apply.
- **Old UI state visible (wrong tab, stale counts, but fully painted)** →
  Data refresh problem, not a render problem. Look at the gui-refresh
  timer's `visible_for_gui` gate, the snapshot buffer, or the config
  read path. Do **not** apply Steps 1–3 to this.
- **Window completely missing / not on screen** → Different bug entirely.
  Check tray show-handler and `window_visible` AtomicBool.
