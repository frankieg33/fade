# Phase 2 — Design

**Status:** Approved
**Date:** 2026-04-22
**Scope:** Rename, icon picker, move-app, restore-defaults, search-filter bug.

## Goal

Ship the deferred phase-2 feature set: let users rename groups, change icons on groups and apps (via a searchable glyph picker), move apps between groups, wipe everything back to defaults from General Settings — and fix the inert search bar while we're in there.

## Data Model

Additive serde-defaulted fields. Old configs load unchanged.

```rust
pub struct Bucket {
    // existing fields...
    #[serde(default)]
    pub icon: Option<String>,    // NEW — Nerd Font glyph; None = look up by name
}

pub struct AppRule {
    // existing fields...
    #[serde(default)]
    pub icon: Option<String>,    // NEW
}
```

Processes listed in a bucket are plain strings, not `AppRule`s. To allow per-process icon overrides for a process that's NOT customized (no rule), we piggy-back on `AppRule`: changing an icon auto-creates a minimal `AppRule` for that process so the override can be stored. The existing `customized` flag in the Slint model stays as-is — icon override alone does not make an app count as "customized" in the UI (no inline slider/combo). To keep this clean, introduce a second signal:

```rust
// GroupAppModel already has `customized: bool`
// Add: app-row looks up icon via config.icon_for_app(group_idx, process) helper
```

**Helper (Rust-side):**
```rust
impl Config {
    pub fn icon_for_app(&self, process: &str) -> String {
        self.app_rule.iter()
            .find(|r| r.process.eq_ignore_ascii_case(process))
            .and_then(|r| r.icon.clone())
            .unwrap_or_else(|| icons::process_icon(process).to_string())
    }
    pub fn icon_for_bucket(&self, idx: usize) -> String {
        self.bucket.get(idx)
            .and_then(|b| b.icon.clone())
            .unwrap_or_else(|| icons::bucket_icon(&self.bucket[idx].name).to_string())
    }
}
```

`build_groups()` and `build_unassigned_rules()` call these instead of `icons::*_icon()` directly.

## Feature 1: Rename Group

**UI:** Pencil glyph `\u{F03EB}` sits between name and timeout slider in the group header. Click → name Text becomes a `LineEdit` (same row, no layout shift). Enter or blur → commit; Esc → revert.

Slint pattern:
```slint
property <int> renaming-idx: -1;  // -1 = none, otherwise group idx being renamed

// In the group header, replace static name Text with:
if root.renaming-idx != g-idx: Text { text: group.name; ... }
if root.renaming-idx == g-idx: LineEdit {
    text: group.name;
    accepted(val) => {
        root.rename-group(g-idx, val);
        root.renaming-idx = -1;
    }
    // blur via FocusScope edit_accepted or similar — details in plan
}
```

**Callback:** `rename-group(int, string)` → Rust trims, rejects empty, writes `bucket[idx].name`, saves, refreshes.

**Edge case:** Two groups with the same name — allowed. Icon lookup by name still works; `resolve_process` uses process names not group names. No uniqueness constraint.

## Feature 2: Icon Picker (groups + apps)

**Data source:** A curated list of ~500 Nerd Font Material Design Icons (codepoint + search keywords) hardcoded in a new `src/icon_catalog.rs`. Generated from the Nerd Font MDI subset we already bundle (`SymbolsNerdFontMono-Regular.ttf`). Each entry: `{ glyph: &'static str, keywords: &'static [&'static str] }`.

Why hardcoded: simpler than parsing the font at runtime, no new deps, search runs against string keywords.

Initial catalog size: aim for ~500 — covers common categories (web, chat, media, dev, games, office, file, system, shapes, arrows, weather, transport, hardware). Easy to grow later by appending entries.

**UI:** Click the group icon or app-row icon → floating picker opens (like the existing Active Processes drawer pattern but centered). Picker contains:
- Search `LineEdit` at top ("Search icons…")
- ScrollView with a grid of glyph buttons (8 per row, ~40px cells)
- Click glyph → selects it, closes picker, fires callback

Search matches against `keywords` (case-insensitive substring).

**State:**
```slint
in-out property <int> icon-picker-group: -1;      // group idx, -1 = closed
in-out property <int> icon-picker-app: -1;        // app idx, -1 = not-an-app pick
in-out property <string> icon-search: "";
in property <[IconEntry]> icon-catalog;
```

`IconEntry { glyph: string, keywords: string /* space-joined for simple contains() */ }`.

**Callbacks:** `set-group-icon(int, string)`, `set-app-icon(int, int, string)`. When `set-app-icon` fires for an app that doesn't yet have an `AppRule`, Rust creates one with the bucket's current timeout/action (so resolution stays identical) AND sets `customized = false` in the UI sense — i.e., we don't want the inline slider to appear just because the icon changed.

**Critical trade-off:** Introducing an AppRule silently for an icon-only change means `customized` in `GroupAppModel` could flip unexpectedly. Fix: compute `customized` in `build_groups` as "has an AppRule with non-default timeout OR non-default action", not "has an AppRule at all". Default = group's current timeout/action.

```rust
fn app_is_customized(bucket: &Bucket, rule: &AppRule) -> bool {
    rule.timeout_mins != bucket.timeout_mins || rule.action != bucket.action
}
```

Apps with icon-only overrides still show "Edit" button (not inline slider); clicking "Edit" sets `customized = true` by diverging timeout/action from group.

## Feature 3: Move App Between Groups

**UI:** Small kebab glyph (`\u{F01D9}` three-dots) at the right end of each app row, next to the Edit/Reset button. Click → floating menu anchored near the kebab:

```
┌────────────────────┐
│ Move to group:     │
│   Browsing         │
│   Communication    │ ← current group dimmed
│   Media            │
│   ...              │
│ ─────────────────  │
│ Remove from group  │
└────────────────────┘
```

**Callbacks:**
- `move-app(from_g, app_idx, to_g)` — removes process from `bucket[from_g].processes`, appends to `bucket[to_g].processes`. Preserves the `AppRule` (icon, timeout, action, enabled) intact — it still applies by process name.
- `remove-from-group(from_g, app_idx)` — removes from bucket only. If there's an `AppRule`, it becomes an "unassigned rule" and shows in that section. If there's no rule, the app disappears entirely.

**State:**
```slint
in-out property <int> kebab-group: -1;
in-out property <int> kebab-app: -1;
```

Menu renders when `kebab-group == g-idx && kebab-app == a-idx`. Click outside via invisible full-window TouchArea with lower z — covers the "click elsewhere to dismiss" case.

## Feature 4: Restore to Defaults

**UI:** In General Settings page, add a card at the bottom:

```
┌─ Danger Zone ─────────────────────────────────────┐
│                                                    │
│ Restore Defaults                                   │
│ Wipes all custom rules, added processes, and      │
│ reverts groups to factory settings. Cannot undo.  │
│                                                    │
│                           [  Restore Defaults  ]  │
└────────────────────────────────────────────────────┘
```

**Confirm flow:** Click → the button itself morphs into "Are you sure? [Yes] [No]" in-place (no modal dialog). Yes → `restore-defaults()` callback → Rust replaces `*config = Config::default_config()`, saves, refreshes. No / click elsewhere → revert button.

**State:**
```slint
property <bool> confirming-reset: false;
```

**Callback:** `restore-defaults()` — wipes config to `Config::default_config()`. Preserves `general.hidden_processes`? **Decision: no, full wipe** per user direction. Auto-start state preservation → also reset (matches `General::default()`).

## Feature 5: Search Filter (bug fix)

**Problem:** `search-text` is bound but unused.

**Design:** Filter groups + apps in Slint (client-side, instant). Slint supports basic string operations — use `to-lowercase()` and check substring via a helper, or add a `filter()` function. Simplest approach: add `filtered` flag per app in Rust and rebuild on every search change.

**Chosen:** Slint-side filtering to avoid a rebuild cycle on every keystroke. Wrap each app row in `if` that tests name match, and each group in `if` that tests whether any of its apps match (or the group name itself matches).

Slint string functions available: `.to-lowercase()`, `.contains()`. Confirmed supported via Slint docs.

Empty search → everything shown (current behavior).

```slint
property <string> search-lc: root.search-text.to-lowercase();

// Each app row:
if root.search-lc == "" || app.process.to-lowercase().contains(root.search-lc): Rectangle { ... }

// Each group card: wrap in if root.search-lc == "" || group-matches-search
// Where group-matches = name matches OR any app matches
```

Computing "any app matches" in Slint requires iteration. Alternative: pre-compute in Rust — when search changes, rebuild `groups` with a `visible` flag per group and per app. Slight perf cost on keystroke but trivial at <100 apps.

**Picked:** Rust-side rebuild via a new `search-changed(string)` callback that stores `current_search` in a shared cell and re-runs `update_gui_from_config()`. Cleanest, avoids Slint gymnastics.

## Implementation Order

1. Data-model additions (`icon: Option<String>` on Bucket + AppRule, `icon_for_*` helpers, `app_is_customized`).
2. Rename groups (inline LineEdit + callback).
3. Icon catalog module + picker UI + callbacks (groups first, then apps).
4. Move-app kebab menu + callbacks.
5. Restore-defaults card + confirm flow.
6. Search filter (Rust-side rebuild).
7. Cross-platform check (`cargo test` + Windows cross-build).

## Non-Goals

- True drag-and-drop (deferred to a later phase if the menu proves insufficient).
- Custom glyphs outside the bundled Nerd Font.
- Import/export of configs.
- Per-app rename (apps are identified by process name; renaming would be misleading).

## Testing

- Unit: serde defaults for new icon fields; roundtrip with `icon = Some(...)`; `app_is_customized` boundary cases; `icon_for_app` fallback.
- Visual on Windows:
  - Rename: pencil click → edit → Enter commits; icon unchanged after rename.
  - Icon picker: search filters; selection updates the correct target.
  - Move: kebab menu, process moves, AppRule preserved, unassigned section populates when removing.
  - Restore: confirm flow, full wipe, fresh defaults load.
  - Search: typing filters groups and apps live.
