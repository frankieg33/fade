# UI Visual Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix four visual/interaction issues — persisted collapse, search bar padding, inline custom-rule controls, and drawer text readability.

**Architecture:** Config gains a single `expanded: bool` per Bucket. Slint's `GroupModel` gains a matching property wired through `build_groups()`. The collapse toggle moves from a small pencil icon to a header-wide TouchArea with a chevron glyph. Three other fixes are Slint-layout-only changes with no Rust-side impact.

**Tech Stack:** Rust, Slint 1.x, TOML config via `serde`

---

### Task 1: Add `expanded` field to Bucket config

**Files:**
- Modify: `src/config.rs:66-76` (Bucket struct)
- Modify: `src/config.rs:221-288` (default_buckets)
- Modify: `src/config.rs:290-470` (tests)

- [ ] **Step 1: Write the failing test**

Add this test at the bottom of the `mod tests` block in `src/config.rs`:

```rust
#[test]
fn test_expanded_defaults_to_true_from_old_toml() {
    // Simulates loading a config file that was saved before the `expanded` field existed
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
    assert!(config.bucket[0].expanded, "expanded should default to true for old configs");
}

#[test]
fn test_expanded_roundtrip() {
    let mut config = Config::default_config();
    config.bucket[0].expanded = false;
    let serialized = toml::to_string_pretty(&config).unwrap();
    let deserialized: Config = toml::from_str(&serialized).unwrap();
    assert!(!deserialized.bucket[0].expanded, "expanded=false should survive roundtrip");
    assert!(deserialized.bucket[1].expanded, "other buckets should stay expanded");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_expanded_defaults_to_true_from_old_toml test_expanded_roundtrip`
Expected: FAIL — `Bucket` has no field `expanded`

- [ ] **Step 3: Add expanded field to Bucket struct**

In `src/config.rs`, add the `expanded` field to the `Bucket` struct (after the `enabled` field at line 76):

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
    pub expanded: bool,
}
```

- [ ] **Step 4: Update default_buckets() to set expanded: true**

In `src/config.rs`, in the `default_buckets()` function (starting at line 221), add `expanded: true` to every `Bucket` literal. For example, the Browsing bucket becomes:

```rust
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
    enabled: false,
    expanded: true,
},
```

Repeat for all 5 buckets: Browsing, Communication, Media, Development, Gaming.

Also update any test that constructs `Bucket` literals directly. There are several in the tests module. Each `Bucket { ... }` literal needs `expanded: true` (or `expanded: false` where appropriate for testing) to compile. The affected tests that construct Bucket literals are:

- `test_resolve_app_rule_priority` (line 303)
- `test_resolve_bucket_fallback` (line 327)
- `test_resolve_disabled_bucket_skipped` (line 367)

Add `expanded: true,` to each of those Bucket literals.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: ALL tests pass, including the two new ones

- [ ] **Step 6: Commit**

```bash
git add src/config.rs
git commit -m "feat: add persisted expanded field to Bucket config"
```

---

### Task 2: Wire collapse through Slint model and Rust callbacks

This task spans both `ui/main.slint` and `src/main.rs` — all three changes must be applied together for the build to succeed.

**Files:**
- Modify: `ui/main.slint:16-23` (GroupModel struct)
- Modify: `ui/main.slint:70-74` (callbacks section)
- Modify: `ui/main.slint:266-352` (group card header + collapse toggle)
- Modify: `src/main.rs:198-227` (build_groups)
- Modify: `src/main.rs:302-355` (setup_gui_callbacks, group section)

- [ ] **Step 1: Add `expanded` to GroupModel in Slint**

In `ui/main.slint`, add `expanded: bool` to the `GroupModel` struct (after the `apps` field at line 22):

```slint
export struct GroupModel {
    icon: string,
    name: string,
    enabled: bool,
    timeout-mins: int,
    action: string,
    apps: [GroupAppModel],
    expanded: bool,
}
```

- [ ] **Step 2: Add toggle-group-expanded callback**

In `ui/main.slint`, after the `add-app-to-group` callback (line 74), add:

```slint
callback toggle-group-expanded(int /* group idx */);
```

- [ ] **Step 3: Replace local expanded property with model property**

In `ui/main.slint`, in the group card `Rectangle` at line 266, remove the local property:

Delete this line (line 272):
```slint
property <bool> expanded: true;
```

Then update all references to `expanded` within this card to use `group.expanded` instead. There are three `if expanded:` conditions in the card body:

- Line 356: `if expanded: Rectangle {` → `if group.expanded: Rectangle {`
- Line 377: `if expanded: VerticalBox {` → `if group.expanded: VerticalBox {`

- [ ] **Step 4: Replace pencil icon with chevron and make header clickable**

Replace the entire header `Rectangle` block at lines 279-352 with the following. Key changes: (a) wrap header contents in a TouchArea that fires `toggle-group-expanded`, (b) swap pencil glyph `\u{F03EB}` for chevron-down `\u{F0140}` / chevron-right `\u{F0142}`:

```slint
// Card header
Rectangle {
    height: 44px;
    border-radius: FadeTheme.radius;
    background: header-ta.has-hover ? FadeTheme.card-header-bg : FadeTheme.card-header-bg;

    header-ta := TouchArea {
        mouse-cursor: pointer;
        clicked => { root.toggle-group-expanded(g-idx); }
    }

    HorizontalBox {
        padding-left: 12px;
        padding-right: 12px;
        spacing: 8px;

        CheckBox {
            checked: group.enabled;
            toggled => { root.toggle-group(g-idx, self.checked); }
        }
        Text {
            text: group.icon;
            font-family: "Symbols Nerd Font Mono";
            font-size: 16px;
            color: FadeTheme.accent;
            vertical-alignment: center;
            width: 22px;
        }
        Text {
            text: group.name;
            color: FadeTheme.text;
            font-size: 14px;
            font-weight: 700;
            vertical-alignment: center;
            horizontal-stretch: 1;
        }
        // Group timeout
        Text {
            text: Math.round(grp-slider.value) * 5 + " min";
            color: FadeTheme.text;
            font-size: 12px;
            width: 48px;
            horizontal-alignment: right;
            vertical-alignment: center;
        }
        grp-slider := Slider {
            width: 100px;
            minimum: 1;
            maximum: 24;
            value: Math.ceil(group.timeout-mins / 5);
            changed(val) => {
                root.update-group-timeout(g-idx, Math.round(val) * 5);
            }
        }
        ComboBox {
            width: 95px;
            model: ["minimize", "close"];
            current-value: group.action;
            selected(val) => { root.update-group-action(g-idx, val); }
        }
        // Chevron
        Text {
            text: group.expanded ? "\u{F0140}" : "\u{F0142}";
            font-family: "Symbols Nerd Font Mono";
            font-size: 14px;
            color: FadeTheme.text-dim;
            vertical-alignment: center;
            width: 24px;
            horizontal-alignment: center;
        }
    }
}
```

Note: The `header-ta` TouchArea sits behind the `HorizontalBox`. Slint's interactive child widgets (CheckBox, Slider, ComboBox) consume their own pointer events and do not propagate to the parent TouchArea, so clicking them will NOT trigger the collapse toggle. This is the standard Slint event model.

- [ ] **Step 5: Update build_groups() in Rust**

In `src/main.rs`, in the `build_groups()` function (line 199), add `expanded` to the `GroupModel` struct literal:

```rust
fn build_groups(config: &Config) -> Vec<GroupModel> {
    config.bucket.iter().map(|bucket| {
        let apps: Vec<GroupAppModel> = bucket.processes.iter().map(|proc| {
            let customized = find_app_rule(config, proc).is_some();
            let (enabled, timeout, action) = if let Some(rule) = find_app_rule(config, proc) {
                (rule.enabled, rule.timeout_mins as i32, rule.action.as_str().into())
            } else {
                (bucket.enabled, bucket.timeout_mins as i32, bucket.action.as_str().into())
            };
            GroupAppModel {
                icon: icons::process_icon(proc).into(),
                process: proc.clone().into(),
                customized,
                enabled,
                timeout_mins: timeout,
                action,
            }
        }).collect();

        GroupModel {
            icon: icons::bucket_icon(&bucket.name).into(),
            name: bucket.name.clone().into(),
            enabled: bucket.enabled,
            timeout_mins: bucket.timeout_mins as i32,
            action: bucket.action.as_str().into(),
            apps: std::rc::Rc::new(slint::VecModel::from(apps)).into(),
            expanded: bucket.expanded,
        }
    }).collect()
}
```

- [ ] **Step 6: Wire toggle-group-expanded callback**

In `src/main.rs`, in `setup_gui_callbacks()`, after the `on_update_group_action` block (ending at line 355), add:

```rust
let cfg = config.clone();
let weak = window.as_weak();
let snap = snapshot_buffer.clone();
window.on_toggle_group_expanded(move |idx| {
    if let Ok(mut c) = cfg.write() {
        let idx = idx as usize;
        if idx < c.bucket.len() {
            c.bucket[idx].expanded = !c.bucket[idx].expanded;
            let _ = c.save();
            refresh_all(&c, &weak, &snap);
        }
    }
});
```

- [ ] **Step 7: Build and test**

Run: `cargo test && cargo build --target x86_64-pc-windows-gnu`
Expected: All tests pass, build succeeds

- [ ] **Step 8: Commit**

```bash
git add src/main.rs ui/main.slint
git commit -m "feat: persisted collapse with header-wide click and chevron glyph"
```

---

### Task 3: Fix search bar padding

**Files:**
- Modify: `ui/main.slint:224-249` (search box)

- [ ] **Step 1: Replace the search box layout**

In `ui/main.slint`, replace the search box block at lines 224-249 with:

```slint
// Search box
Rectangle {
    width: 220px;
    height: 30px;
    background: FadeTheme.input-bg;
    border-radius: FadeTheme.radius;
    border-width: 1px;
    border-color: FadeTheme.border;
    HorizontalBox {
        padding: 0px;
        spacing: 0px;
        Rectangle {
            width: 32px;
            Text {
                text: "\u{F0349}";
                font-family: "Symbols Nerd Font Mono";
                font-size: 13px;
                color: FadeTheme.text-dim;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
        search-input := LineEdit {
            horizontal-stretch: 1;
            placeholder-text: "Search apps...";
            text <=> root.search-text;
        }
    }
}
```

Key changes from original:
- Outer width bumped from `180px` to `220px` to accommodate the dedicated icon slot.
- `HorizontalBox` padding and spacing set to `0px` — no competing padding.
- Search icon is inside its own `Rectangle { width: 32px; }` container, so it has a fixed slot and never overlaps the `LineEdit`.

- [ ] **Step 2: Build to verify**

Run: `cargo build --target x86_64-pc-windows-gnu`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add ui/main.slint
git commit -m "fix: search bar icon no longer overlaps placeholder text"
```

---

### Task 4: Custom rule controls inline

**Files:**
- Modify: `ui/main.slint:41-47` (window properties — min-width)
- Modify: `ui/main.slint:383-516` (app row within group card)

- [ ] **Step 1: Raise min-width**

In `ui/main.slint`, change line 46 from:

```slint
min-width: 600px;
```

to:

```slint
min-width: 720px;
```

- [ ] **Step 2: Replace the app row block**

In `ui/main.slint`, replace the entire `for app[a-idx] in group.apps: Rectangle { ... }` block (lines 383 through 513) with the following single-row layout:

```slint
for app[a-idx] in group.apps: Rectangle {
    height: 34px;
    border-radius: 4px;
    background: app-row-ta.has-hover ? FadeTheme.surface-hover : FadeTheme.surface;
    app-row-ta := TouchArea {}

    HorizontalBox {
        height: 34px;
        padding-left: 8px;
        padding-right: 8px;
        spacing: 6px;

        CheckBox {
            checked: app.enabled;
            toggled => { root.toggle-app(g-idx, a-idx, self.checked); }
        }
        Text {
            text: app.icon;
            font-family: "Symbols Nerd Font Mono";
            font-size: 13px;
            color: FadeTheme.accent;
            vertical-alignment: center;
            width: 18px;
        }
        Text {
            text: app.process;
            color: FadeTheme.text;
            font-size: 12px;
            vertical-alignment: center;
            width: 120px;
            overflow: elide;
        }

        // Stretchy spacer pushes controls to the right
        Rectangle {
            horizontal-stretch: 1;
        }

        // Inline custom controls — only when customized
        if app.customized: Text {
            text: Math.round(app-slider.value) * 5 + " min";
            color: FadeTheme.text;
            font-size: 11px;
            width: 42px;
            horizontal-alignment: right;
            vertical-alignment: center;
        }
        if app.customized: app-slider := Slider {
            width: 90px;
            minimum: 1;
            maximum: 24;
            value: Math.ceil(app.timeout-mins / 5);
            changed(val) => {
                root.update-app-timeout(g-idx, a-idx, Math.round(val) * 5);
            }
        }
        if app.customized: ComboBox {
            width: 85px;
            model: ["minimize", "close"];
            current-value: app.action;
            selected(val) => { root.update-app-action(g-idx, a-idx, val); }
        }

        // Customize / Reset button — always visible
        if !app.customized: Rectangle {
            width: 90px;
            height: 22px;
            border-radius: 4px;
            background: cust-ta.has-hover ? FadeTheme.surface-hover : FadeTheme.surface;
            border-width: 1px;
            border-color: FadeTheme.border;
            cust-ta := TouchArea {
                mouse-cursor: pointer;
                clicked => { root.customize-app(g-idx, a-idx); }
            }
            Text {
                text: "Customize";
                font-size: 10px;
                color: cust-ta.has-hover ? FadeTheme.accent : FadeTheme.text-dim;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
        if app.customized: Rectangle {
            width: 90px;
            height: 22px;
            border-radius: 4px;
            background: reset-ta.has-hover ? FadeTheme.warning : FadeTheme.surface;
            border-width: 1px;
            border-color: FadeTheme.border;
            reset-ta := TouchArea {
                mouse-cursor: pointer;
                clicked => { root.reset-app-to-group(g-idx, a-idx); }
            }
            Text {
                text: "Reset";
                font-size: 10px;
                color: reset-ta.has-hover ? FadeTheme.bg : FadeTheme.text-dim;
                horizontal-alignment: center;
                vertical-alignment: center;
            }
        }
    }
}
```

Key changes:
- Deleted the `VerticalBox` wrapper and the second `HorizontalBox` row entirely.
- Deleted `"Inheriting Group Settings"` and `"Custom Rule Applied"` status text labels.
- Row height is now constant `34px` regardless of `app.customized`.
- Custom controls (slider, label, combo) appear inline before the trailing button when `app.customized` is true.
- Trailing button is narrower (`90px` vs. `130px`/`110px`) with shorter labels: `"Customize"` / `"Reset"`.

- [ ] **Step 3: Build to verify**

Run: `cargo build --target x86_64-pc-windows-gnu`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add ui/main.slint
git commit -m "feat: inline custom rule controls, drop status labels, raise min-width"
```

---

### Task 5: Fix drawer LineEdit readability

**Files:**
- Modify: `ui/main.slint:850-863` (manual add row in drawer)

- [ ] **Step 1: Replace the manual add row**

In `ui/main.slint`, find the manual add row (around lines 850-885). Replace the block starting with `// Manual add row` through the end of the `HorizontalBox` containing the manual-input and the + button:

```slint
// Manual add row
HorizontalBox {
    spacing: 4px;
    height: 28px;
    manual-input := LineEdit {
        horizontal-stretch: 1;
        placeholder-text: "Add process name";
    }
    Rectangle {
        width: 28px;
        height: 28px;
        border-radius: 4px;
        background: manual-add-ta.has-hover ? FadeTheme.accent : FadeTheme.surface;
        manual-add-ta := TouchArea {
            mouse-cursor: pointer;
            clicked => {
                root.add-process-name(manual-input.text);
                manual-input.text = "";
            }
        }
        Text {
            text: "+";
            font-size: 14px;
            font-weight: 700;
            color: manual-add-ta.has-hover ? FadeTheme.bg : FadeTheme.text;
            horizontal-alignment: center;
            vertical-alignment: center;
        }
    }
}
```

Key change: Removed the wrapper `Rectangle { background: FadeTheme.input-bg; border-width: 1px; border-color: FadeTheme.border; ... }` around the `LineEdit`. The `LineEdit` now renders its own styled chrome directly.

- [ ] **Step 2: Build to verify**

Run: `cargo build --target x86_64-pc-windows-gnu`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add ui/main.slint
git commit -m "fix: drawer add-process text now readable (remove double-wrapper)"
```

---

### Task 6: Full build and test verification

**Files:** None (verification only)

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: All tests pass (including the two new `expanded` tests from Task 1)

- [ ] **Step 2: Release build**

Run: `cargo build --release --target x86_64-pc-windows-gnu`
Expected: Build succeeds

- [ ] **Step 3: Verify no warnings**

Check the cargo output for any warnings. If there are warnings, fix them before proceeding.
