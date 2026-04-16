# UI Visual Fixes — Design Spec

**Status:** Draft
**Date:** 2026-04-16
**Scope:** Four targeted visual refinements to the unified group/app UI introduced in `2026-04-15-unified-ui-redesign.md`.

## Goal

Fix four visual and interaction issues in the Applications & Rules page without changing the underlying group/app data model. Ship as a single coherent pass before the larger feature group (drag-and-drop, rename, icon picker, restore-defaults).

## Non-Goals

- Group renaming, drag-and-drop, icon customization, restore-to-defaults — these are explicitly deferred to a separate spec/plan.
- Any change to `Monitor`, `WindowApi`, `ResolvedRule`, or the Win32 layer.
- Any change to `AppRule` or `Bucket` *except* the single additive field described in Fix 1.

---

## Fix 1: Collapsible Group Sections (Persisted)

### Problem

Group cards are already collapsible via a pencil glyph (`nf-md-pencil`, codepoint `F03EB`) at `ui/main.slint:343-350`. Two issues:

1. The click target is a small 24×24 square on the right edge — easy to miss.
2. The glyph is a pencil, which strongly implies "edit/rename" rather than "collapse." The rename feature is coming in the next pass, so the pencil needs to be freed.
3. Collapse state is held in a local `expanded` Slint property and is lost whenever the Slint model is rebuilt by `update_gui_from_config()` — effectively resetting every time the user toggles anything.

### Design

**Data model change** — add a single field to `Bucket` in `src/config.rs`:

```rust
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
    pub expanded: bool,   // NEW
}
```

`#[serde(default = "default_true")]` ensures older config files (pre-field) load with all groups expanded. Field literals in `default_buckets()` also set `expanded: true`.

**Slint model change** — `GroupModel` gains a `bool expanded` property wired from `bucket.expanded` in `build_groups()`.

**Callback** — add `toggle-group-expanded(g-idx)` to `SettingsWindow`, wired in `setup_gui_callbacks` to flip `config.bucket[g].expanded` and save.

**UI behavior:**
- The header row (the existing 40px bar: checkbox + icon + name + timeout slider + action combo + collapse button) becomes the click target for toggling. Checkbox, timeout slider, and action combo must still receive their own clicks — they are nested interactive widgets, and `TouchArea.clicked` in Slint does not fire when a child widget handles the click, so no explicit stop-propagation is needed.
- The pencil glyph is replaced with a chevron: `nf-md-chevron-down` (`F0140`) when expanded, `nf-md-chevron-right` (`F0142`) when collapsed.
- Clicking the header anywhere *outside* the interactive child widgets fires `toggle-group-expanded`.

### Trade-offs

- Persisting to config means opening and closing the app preserves collapse state — desirable for users with many groups. Alternative (session-only state in a separate `Rc<RefCell<Vec<bool>>>`) avoids polluting the config but loses state on window close, which is jarring given lazy-create was reverted.
- The header-row click approach depends on Slint's event propagation rules. If testing reveals a conflict (e.g., slider drags triggering the toggle), fallback is to keep the toggle on a dedicated chevron button with a wider hit box.

---

## Fix 2: Search Bar Padding

### Problem

At `ui/main.slint:231-247`, the search row places a search-icon Text element next to a `LineEdit` inside a shared `HorizontalBox`. The `LineEdit` renders its own internal padding, and in the current layout the icon visually sits flush against (or over) the placeholder text, per the screenshot.

### Design

Restructure the search container so the icon and the edit each get their own well-defined space:

```slint
HorizontalBox {
    padding: 0px;
    spacing: 0px;
    Rectangle {
        width: 32px;
        // search glyph centered here
    }
    search-input := LineEdit { horizontal-stretch: 1; placeholder-text: "Search apps..."; }
}
```

The surrounding decorative `Rectangle` (the rounded search "chip" background) becomes the container; the icon gets a fixed 32px slot; the `LineEdit` fills the rest. The `LineEdit`'s internal padding no longer fights the icon.

### Trade-offs

- No change to search semantics; pure layout fix.
- `LineEdit` still uses its own skin/chrome. That matches the drawer fix below and keeps us consistent.

---

## Fix 3: Custom Rule Controls Inline

### Problem

When an app is customized (i.e., an `AppRule` exists for its process), the current UI renders a second row below the main app row (`ui/main.slint:460-511`) containing: timeout label + slider + action combo + reset button. This doubles vertical height (34 → 64px) and wastes horizontal space on the app row.

### Design

Collapse to a single row. Layout from left to right:

```
[✓ checkbox] [icon] [process-name]   [slider + label + action combo]?   [Customize | Reset]
```

Where `?` means "only when `app.customized`".

**Component details:**

- **Status text dropped** — `"Inheriting Group Settings"` (lines 422-428) and `"Custom Rule Applied"` (lines 429-435) are deleted. The presence or absence of the inline controls communicates state adequately.
- **Process name** stays `width: 120px; overflow: elide`. A stretchy `Rectangle { horizontal-stretch: 1; }` sits between the name and the rightmost cluster so the row always fills width.
- **Customized cluster** (visible only when `app.customized`): `label + slider + combo`, the same widgets from the current second row, packed into fixed widths totalling ~230px. No wrapping, no multi-line.
- **Trailing button**: still exists, becomes narrower (`~90px`). Label is `"Customize"` when uncustomized, `"Reset"` when customized. Callback wiring unchanged (`customize-app` vs. `reset-app-to-group`).

Row height becomes constant 34px regardless of `app.customized`.

### Trade-offs

- Total horizontal budget on a customized row: checkbox(24) + icon(18) + process(120) + stretch + label(42) + slider(90) + combo(85) + button(90) + ~6×8px spacing (48) = ~517px fixed width. The current window has `preferred-width: 800px` and `min-width: 600px` (`ui/main.slint:44-46`), minus a 200px sidebar, leaving **600px/400px** of content width. At `min-width` the customized row (~517px fixed) would clip.
- **Mitigation:** raise `min-width` to `720px` so content ≥ 520px, comfortably fitting the customized row. `preferred-width` stays `800px`. This is a minor tradeoff for users on very small screens, but the window is a settings panel, not a primary-use surface.
- Dropping the status text loses a small amount of explicitness. Acceptable: the Customize/Reset label plus inline controls are unambiguous.

---

## Fix 4: Drawer "Add Process" Text Unreadable

### Problem

At `ui/main.slint:854-863`, the manual-add `LineEdit` is wrapped in a custom-styled `Rectangle` that sets `background: FadeTheme.input-bg` and border tokens. Slint's `LineEdit` is a styled widget with its own background/padding skin — nesting it inside another styled container produces a double-background effect where our dark `input-bg` sits behind the widget's own (also dark) chrome, and the widget's default text color is chosen against *its* skin, not ours. Result: text on the drawer is effectively invisible.

### Design

Drop the wrapper:

```slint
HorizontalBox {
    spacing: 4px;
    height: 28px;
    manual-input := LineEdit {
        horizontal-stretch: 1;
        placeholder-text: "Add process name";
    }
    Rectangle {  // + button — unchanged
        width: 28px;
        ...
    }
}
```

`LineEdit` manages its own background, border, text color, and padding via the Slint widget style. The drawer now shows a correctly-contrasted input with readable placeholder and text.

### Trade-offs

- Visual consistency with the search bar (Fix 2) — same pattern, same rationale.
- We lose the custom dark `input-bg` look. Acceptable: the native `LineEdit` skin is readable, and we can revisit with a fully custom `TextInput`-based component later if the native look clashes.

---

## Implementation Order

Sequence within the plan:

1. **Config schema + default** — add `Bucket.expanded: bool` with serde defaults; update `default_buckets()`; add a test that a pre-existing TOML without the field loads with `expanded = true`.
2. **Slint model + callback plumbing** — extend `GroupModel`, add the `toggle-group-expanded` callback, wire in `build_groups()` and `setup_gui_callbacks()`.
3. **Collapse UI** — swap pencil glyph for chevron, make header-row TouchArea the toggle target, replace local `expanded` property with `group.expanded` binding.
4. **Search padding** — restructure search row layout.
5. **Custom rule inline** — delete second-row block; delete status text labels; relayout main row with conditional customized cluster; constant height. Raise `SettingsWindow.min-width` from `600px` to `720px`.
6. **Drawer LineEdit** — remove wrapper Rectangle.
7. **Cross-platform check** — `cargo test` (Linux) + `cargo build --release --target x86_64-pc-windows-gnu` (Windows cross).

## Testing

- Rust: new unit test in `src/config.rs` verifying `expanded` serde default and `default_buckets()` default (`expanded == true`).
- Rust: `cargo test` — all existing tests must still pass (no behavioural change to `resolve_process`, etc.).
- Visual: on Windows, confirm
  - Clicking the group header row toggles collapse; chevron flips direction.
  - Collapse state survives app restart.
  - Slider drag on header does not fire collapse.
  - Checkbox toggle on header does not fire collapse.
  - Search icon no longer overlaps placeholder.
  - Customized app rows are single-height with inline slider/combo/reset.
  - Drawer "Add process name" text is readable.

## Migration / Compatibility

- Config change is additive with serde default. Existing `fade.toml` files load unchanged and gain `expanded = true` on first save.
- No change to rule resolution, monitor, or tray.

## Open Questions

None at design time. Any issues surfaced during implementation (e.g., header-row propagation conflict) get reported as `DONE_WITH_CONCERNS` per the subagent workflow.
