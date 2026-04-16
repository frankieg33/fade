# Unified UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the three-tab UI (Rules / Active Windows / Buckets) with a two-panel layout: a sidebar with "Applications & Rules" and "General Settings" navigation, and a main content area showing a unified hierarchical view of groups (buckets) containing their applications, with an "Active Processes" drawer for adding new rules.

**Architecture:** The new layout uses a sidebar navigation instead of TabWidget. The main "Managed Applications" view merges buckets and app rules into a single hierarchical list: group cards (buckets) contain application rows that inherit group settings by default but can be individually customized. App rules that don't belong to any bucket appear in a special "Individual Rules (Unassigned)" group at the bottom. An "Active Processes" floating panel in the bottom-right replaces the old Active Windows tab, providing quick "add" access to currently-running windows. The config data model (`Config`, `Bucket`, `AppRule`) stays unchanged — the UI merges them visually but the TOML file keeps its current structure.

**Tech Stack:** Slint 1.11 (.slint UI), Rust (callbacks/wiring), existing config/monitor infrastructure unchanged.

---

## File Structure

| File | Role | Action |
|------|------|--------|
| `ui/main.slint` | Main Slint UI — full rewrite of layout | **Rewrite** |
| `ui/style.slint` | Theme tokens — add sidebar + drawer tokens | **Modify** |
| `src/main.rs` | Rust wiring — update callbacks/models for new UI | **Modify** |
| `src/config.rs` | Config structs — unchanged | No change |
| `src/monitor.rs` | Monitor loop — unchanged | No change |
| `src/icons.rs` | Icon mappings — unchanged | No change |

**Key design decision:** The config model stays the same. `app_rule` entries are "customized" when a matching process exists in a bucket's process list AND also has an `app_rule`. The UI shows "Inheriting Group Settings" vs "Custom Rule Applied" based on whether an `app_rule` exists for that process. "Reset to Bucket" simply deletes the `app_rule`, reverting to bucket inheritance.

---

### Task 1: Add new theme tokens to style.slint

**Files:**
- Modify: `ui/style.slint`

- [ ] **Step 1: Add sidebar and drawer theme tokens**

Replace the entire contents of `ui/style.slint` with:

```slint
// Shared design tokens for Fade
export global FadeTheme {
    // Core palette
    out property <color> bg: #1e1e2e;
    out property <color> surface: #313244;
    out property <color> surface-hover: #45475a;
    out property <color> text: #cdd6f4;
    out property <color> text-dim: #a6adc8;
    out property <color> accent: #89b4fa;
    out property <color> danger: #f38ba8;
    out property <color> success: #a6e3a1;
    out property <color> warning: #f9e2af;
    out property <color> border: #585b70;
    out property <length> radius: 6px;
    out property <length> spacing: 8px;

    // Input fields
    out property <color> input-bg: #181825;

    // Sidebar
    out property <color> sidebar-bg: #181825;
    out property <color> sidebar-active: #313244;
    out property <color> sidebar-text: #a6adc8;
    out property <color> sidebar-active-text: #89b4fa;
    out property <length> sidebar-width: 200px;

    // Group cards
    out property <color> card-bg: #262637;
    out property <color> card-header-bg: #2a2a3d;

    // Drawer (Active Processes panel)
    out property <color> drawer-bg: #1a1a2e;
    out property <color> drawer-border: #585b70;

    // Status bar
    out property <color> status-bar-bg: #11111b;
    out property <color> status-bar-text: #a6adc8;
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | tail -5`
Expected: Compiles successfully (style.slint is imported by main.slint, tokens unused yet is fine).

- [ ] **Step 3: Commit**

```bash
git add ui/style.slint
git commit -m "$(cat <<'EOF'
feat: add sidebar and drawer theme tokens for UI redesign
EOF
)"
```

---

### Task 2: Rewrite Slint data models and callbacks

The new UI needs different data models. The old `AppRuleModel`, `ActiveWindowModel`, and `BucketModel` are replaced with models that support the unified hierarchical view.

**Files:**
- Modify: `ui/main.slint` (top section — structs and callback declarations)

- [ ] **Step 1: Replace the data model structs and callback declarations**

Replace everything in `ui/main.slint` from line 1 through the end of the callback declarations (line 60) with:

```slint
import { FadeTheme } from "style.slint";
import { Button, VerticalBox, HorizontalBox, LineEdit, ComboBox, CheckBox, Slider, ScrollView } from "std-widgets.slint";
import "SymbolsNerdFontMono-Regular.ttf";

// An application row inside a group card
export struct GroupAppModel {
    icon: string,           // Nerd Font glyph
    process: string,        // e.g. "chrome.exe"
    customized: bool,       // true if an app_rule overrides the group
    enabled: bool,          // from app_rule if customized, else bucket.enabled
    timeout-mins: int,      // from app_rule if customized, else bucket value
    action: string,         // from app_rule if customized, else bucket value
}

// A group card (bucket)
export struct GroupModel {
    icon: string,           // Nerd Font bucket icon
    name: string,           // e.g. "Browsing"
    enabled: bool,          // bucket.enabled
    timeout-mins: int,      // bucket.timeout_mins
    action: string,         // bucket.action
    apps: [GroupAppModel],  // processes in this bucket
}

// An unassigned app rule (not in any bucket)
export struct UnassignedRuleModel {
    icon: string,
    process: string,
    timeout-mins: int,
    action: string,
    enabled: bool,
}

// Active process in the drawer
export struct ActiveProcessModel {
    icon: string,
    process: string,
    managed: bool,          // already has a rule or is in a bucket
}

export component SettingsWindow inherits Window {
    title: "Fade — Managed Applications";
    icon: @image-url("Fade Icon.png");
    preferred-width: 800px;
    preferred-height: 560px;
    min-width: 600px;
    min-height: 400px;
    background: FadeTheme.bg;

    // Properties bound from Rust
    in property <[GroupModel]> groups;
    in property <[UnassignedRuleModel]> unassigned-rules;
    in property <[ActiveProcessModel]> active-processes;
    in property <int> managed-count: 0;
    in property <int> active-count: 0;
    in property <int> polling-interval-secs: 30;
    in property <bool> auto-start: false;
    in property <string> version: "0.0.0";
    in-out property <bool> paused: false;

    // Navigation state
    in-out property <int> current-page: 0; // 0 = Applications & Rules, 1 = General Settings

    // Search
    in-out property <string> search-text: "";

    // Drawer visibility
    in-out property <bool> drawer-open: false;

    // Group callbacks
    callback toggle-group(int /* group idx */, bool /* enabled */);
    callback update-group-timeout(int /* group idx */, int /* mins */);
    callback update-group-action(int /* group idx */, string /* action */);
    callback add-app-to-group(int /* group idx */, string /* process */);

    // App-in-group callbacks
    callback customize-app(int /* group idx */, int /* app idx */);
    callback reset-app-to-group(int /* group idx */, int /* app idx */);
    callback update-app-timeout(int /* group idx */, int /* app idx */, int /* mins */);
    callback update-app-action(int /* group idx */, int /* app idx */, string /* action */);
    callback toggle-app(int /* group idx */, int /* app idx */, bool /* enabled */);

    // Unassigned rule callbacks
    callback remove-unassigned(int /* idx */);
    callback toggle-unassigned(int /* idx */, bool /* enabled */);
    callback update-unassigned-timeout(int /* idx */, int /* mins */);
    callback update-unassigned-action(int /* idx */, string /* action */);

    // Drawer callbacks
    callback add-rule(string /* process */);
    callback add-process-name(string /* process */);

    // General settings callbacks
    callback set-polling-interval(int /* secs */);
    callback set-auto-start(bool);
    callback hide-process(string /* process */);
```

Note: Do NOT close the component yet — the layout body comes in Task 3.

- [ ] **Step 2: Verify it compiles**

The file won't compile yet because the component body is incomplete. That's expected. Just check that there are no syntax errors in the struct/callback section by looking at the compiler output:

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | grep "error" | head -5`
Expected: Errors about missing closing brace or missing body, NOT about struct/callback syntax.

- [ ] **Step 3: Commit**

```bash
git add ui/main.slint
git commit -m "$(cat <<'EOF'
feat: rewrite Slint data models for unified group/app hierarchy
EOF
)"
```

---

### Task 3: Build the sidebar and main layout skeleton

**Files:**
- Modify: `ui/main.slint` (continue building the component body)

- [ ] **Step 1: Add the sidebar + main content area layout**

After the callback declarations from Task 2, add the layout body. This replaces the old `TabWidget` entirely. Add the following right after the last callback line (`callback set-auto-start(bool);` and `callback hide-process(string);`):

```slint
    // ═══════════════════════════════════════════════
    // MAIN LAYOUT: sidebar + content
    // ═══════════════════════════════════════════════
    HorizontalBox {
        padding: 0px;
        spacing: 0px;

        // ── Sidebar ──
        Rectangle {
            width: FadeTheme.sidebar-width;
            background: FadeTheme.sidebar-bg;

            VerticalBox {
                padding-top: 16px;
                padding-bottom: 8px;
                spacing: 0px;

                // Logo area
                HorizontalBox {
                    padding-left: 16px;
                    padding-bottom: 16px;
                    spacing: 8px;
                    alignment: start;
                    Image {
                        source: @image-url("Fade Icon.png");
                        width: 32px;
                        height: 32px;
                    }
                    Text {
                        text: "Fade";
                        color: FadeTheme.accent;
                        font-size: 20px;
                        font-weight: 700;
                        vertical-alignment: center;
                    }
                }

                // Nav: Applications & Rules
                Rectangle {
                    height: 36px;
                    border-radius: FadeTheme.radius;
                    background: root.current-page == 0 ? FadeTheme.sidebar-active : nav0-ta.has-hover ? FadeTheme.surface-hover : transparent;
                    nav0-ta := TouchArea {
                        mouse-cursor: pointer;
                        clicked => { root.current-page = 0; }
                    }
                    HorizontalBox {
                        padding-left: 12px;
                        spacing: 8px;
                        Text {
                            text: "\u{F0493}";
                            font-family: "Symbols Nerd Font Mono";
                            font-size: 14px;
                            color: root.current-page == 0 ? FadeTheme.sidebar-active-text : FadeTheme.sidebar-text;
                            vertical-alignment: center;
                        }
                        Text {
                            text: "Applications & Rules";
                            font-size: 13px;
                            color: root.current-page == 0 ? FadeTheme.sidebar-active-text : FadeTheme.sidebar-text;
                            vertical-alignment: center;
                        }
                    }
                }

                // Nav: General Settings
                Rectangle {
                    height: 36px;
                    border-radius: FadeTheme.radius;
                    background: root.current-page == 1 ? FadeTheme.sidebar-active : nav1-ta.has-hover ? FadeTheme.surface-hover : transparent;
                    nav1-ta := TouchArea {
                        mouse-cursor: pointer;
                        clicked => { root.current-page = 1; }
                    }
                    HorizontalBox {
                        padding-left: 12px;
                        spacing: 8px;
                        Text {
                            text: "\u{F0493}";
                            font-family: "Symbols Nerd Font Mono";
                            font-size: 14px;
                            color: root.current-page == 1 ? FadeTheme.sidebar-active-text : FadeTheme.sidebar-text;
                            vertical-alignment: center;
                        }
                        Text {
                            text: "General Settings";
                            font-size: 13px;
                            color: root.current-page == 1 ? FadeTheme.sidebar-active-text : FadeTheme.sidebar-text;
                            vertical-alignment: center;
                        }
                    }
                }

                // Spacer
                Rectangle { vertical-stretch: 1; }
            }
        }

        // ── Content area ──
        Rectangle {
            horizontal-stretch: 1;
            background: FadeTheme.bg;

            // Page 0: Applications & Rules
            if root.current-page == 0: VerticalBox {
                padding: 20px;
                spacing: 12px;

                // Header row
                HorizontalBox {
                    spacing: 12px;
                    VerticalBox {
                        spacing: 2px;
                        horizontal-stretch: 1;
                        Text {
                            text: "Managed Applications";
                            color: FadeTheme.text;
                            font-size: 18px;
                            font-weight: 700;
                        }
                        Text {
                            text: root.managed-count + " Applications Managed | " + root.active-count + " Windows Active";
                            color: FadeTheme.text-dim;
                            font-size: 12px;
                        }
                    }
                    // Search box
                    Rectangle {
                        width: 180px;
                        height: 30px;
                        background: FadeTheme.input-bg;
                        border-radius: FadeTheme.radius;
                        border-width: 1px;
                        border-color: FadeTheme.border;
                        HorizontalBox {
                            padding-left: 8px;
                            padding-right: 8px;
                            spacing: 6px;
                            Text {
                                text: "\u{F0349}";
                                font-family: "Symbols Nerd Font Mono";
                                font-size: 13px;
                                color: FadeTheme.text-dim;
                                vertical-alignment: center;
                            }
                            search-input := LineEdit {
                                horizontal-stretch: 1;
                                placeholder-text: "Search apps...";
                                text <=> root.search-text;
                            }
                        }
                    }
                }

                // Section heading
                Text {
                    text: "Combined Rules & Buckets";
                    color: FadeTheme.text-dim;
                    font-size: 13px;
                    font-weight: 600;
                }

                // ── GROUPS scroll area ── (placeholder — Task 4 fills this)
                ScrollView {
                    vertical-stretch: 1;
                    VerticalBox {
                        spacing: 12px;
                        // GROUP CARDS GO HERE (Task 4)
                        // UNASSIGNED RULES GO HERE (Task 5)
                    }
                }

                // ── Bottom bar ──
                HorizontalBox {
                    spacing: 8px;
                    alignment: space-between;
                    HorizontalBox {
                        spacing: 4px;
                        Text {
                            text: "\u{F04CC}";
                            font-family: "Symbols Nerd Font Mono";
                            font-size: 12px;
                            color: FadeTheme.text-dim;
                            vertical-alignment: center;
                        }
                        Text {
                            text: root.managed-count + " managed apps";
                            color: FadeTheme.text-dim;
                            font-size: 12px;
                            vertical-alignment: center;
                        }
                    }
                    Rectangle {
                        width: 180px;
                        height: 32px;
                        border-radius: FadeTheme.radius;
                        background: add-new-ta.has-hover ? FadeTheme.accent : FadeTheme.surface;
                        add-new-ta := TouchArea {
                            mouse-cursor: pointer;
                            clicked => { root.drawer-open = !root.drawer-open; }
                        }
                        HorizontalBox {
                            padding-left: 12px;
                            padding-right: 12px;
                            spacing: 6px;
                            alignment: center;
                            Text {
                                text: "+";
                                color: add-new-ta.has-hover ? FadeTheme.bg : FadeTheme.accent;
                                font-size: 14px;
                                font-weight: 700;
                                vertical-alignment: center;
                            }
                            Text {
                                text: "Add New Managed App";
                                color: add-new-ta.has-hover ? FadeTheme.bg : FadeTheme.text;
                                font-size: 13px;
                                font-weight: 600;
                                vertical-alignment: center;
                            }
                        }
                    }
                }
            }

            // Page 1: General Settings
            if root.current-page == 1: VerticalBox {
                padding: 20px;
                spacing: 16px;

                Text {
                    text: "General Settings";
                    color: FadeTheme.text;
                    font-size: 18px;
                    font-weight: 700;
                }

                HorizontalBox {
                    spacing: FadeTheme.spacing;
                    Text { text: "Polling interval:"; color: FadeTheme.text; vertical-alignment: center; width: 150px; }
                    ComboBox {
                        width: 120px;
                        model: ["15 sec", "30 sec", "60 sec", "120 sec"];
                        current-value: root.polling-interval-secs == 15 ? "15 sec" :
                                       root.polling-interval-secs == 60 ? "60 sec" :
                                       root.polling-interval-secs == 120 ? "120 sec" :
                                       root.polling-interval-secs + " sec";
                        selected(val) => {
                            if (val == "15 sec") { root.set-polling-interval(15); }
                            if (val == "30 sec") { root.set-polling-interval(30); }
                            if (val == "60 sec") { root.set-polling-interval(60); }
                            if (val == "120 sec") { root.set-polling-interval(120); }
                        }
                    }
                }

                HorizontalBox {
                    spacing: FadeTheme.spacing;
                    Text { text: "Start with Windows:"; color: FadeTheme.text; vertical-alignment: center; width: 150px; }
                    CheckBox {
                        checked: root.auto-start;
                        toggled => { root.set-auto-start(self.checked); }
                    }
                }

                Rectangle { vertical-stretch: 1; }

                // Status bar
                Rectangle {
                    height: 28px;
                    background: FadeTheme.status-bar-bg;
                    border-radius: FadeTheme.radius;
                    Text {
                        text: "Fade v" + root.version;
                        color: FadeTheme.status-bar-text;
                        font-size: 11px;
                        horizontal-alignment: center;
                        vertical-alignment: center;
                    }
                }
            }

            // ── Active Processes Drawer (floating overlay) ──
            if root.drawer-open: Rectangle {
                x: parent.width - 240px;
                y: parent.height - 280px;
                width: 230px;
                height: 270px;
                background: FadeTheme.drawer-bg;
                border-radius: FadeTheme.radius;
                border-width: 1px;
                border-color: FadeTheme.drawer-border;
                drop-shadow-blur: 16px;
                drop-shadow-color: #00000066;

                VerticalBox {
                    padding: 10px;
                    spacing: 6px;

                    // Drawer header
                    HorizontalBox {
                        spacing: 4px;
                        Text {
                            text: "Active Processes";
                            color: FadeTheme.text;
                            font-size: 13px;
                            font-weight: 600;
                            horizontal-stretch: 1;
                            vertical-alignment: center;
                        }
                        Rectangle {
                            width: 20px;
                            height: 20px;
                            border-radius: 4px;
                            background: close-drawer-ta.has-hover ? FadeTheme.danger : transparent;
                            close-drawer-ta := TouchArea {
                                mouse-cursor: pointer;
                                clicked => { root.drawer-open = false; }
                            }
                            Text {
                                text: "✕";
                                font-size: 11px;
                                color: close-drawer-ta.has-hover ? FadeTheme.bg : FadeTheme.text-dim;
                                horizontal-alignment: center;
                                vertical-alignment: center;
                            }
                        }
                    }

                    // Active process list
                    ScrollView {
                        vertical-stretch: 1;
                        VerticalBox {
                            spacing: 2px;
                            for proc[idx] in root.active-processes: Rectangle {
                                height: 28px;
                                border-radius: 4px;
                                background: proc-ta.has-hover ? FadeTheme.surface-hover : transparent;
                                proc-ta := TouchArea {}

                                HorizontalBox {
                                    padding-left: 6px;
                                    padding-right: 6px;
                                    spacing: 6px;
                                    Text {
                                        text: proc.icon;
                                        font-family: "Symbols Nerd Font Mono";
                                        font-size: 12px;
                                        color: FadeTheme.accent;
                                        vertical-alignment: center;
                                        width: 16px;
                                    }
                                    Text {
                                        text: proc.process;
                                        color: FadeTheme.text;
                                        font-size: 12px;
                                        vertical-alignment: center;
                                        horizontal-stretch: 1;
                                        overflow: elide;
                                    }
                                    if !proc.managed: Rectangle {
                                        width: 32px;
                                        height: 20px;
                                        border-radius: 4px;
                                        background: add-proc-ta.has-hover ? FadeTheme.accent : FadeTheme.surface;
                                        add-proc-ta := TouchArea {
                                            mouse-cursor: pointer;
                                            clicked => { root.add-rule(proc.process); }
                                        }
                                        Text {
                                            text: "add";
                                            font-size: 10px;
                                            color: add-proc-ta.has-hover ? FadeTheme.bg : FadeTheme.text;
                                            horizontal-alignment: center;
                                            vertical-alignment: center;
                                        }
                                    }
                                    if proc.managed: Text {
                                        text: "+";
                                        font-size: 12px;
                                        color: FadeTheme.text-dim;
                                        width: 32px;
                                        horizontal-alignment: center;
                                        vertical-alignment: center;
                                    }
                                }
                            }
                        }
                    }

                    // Manual add row
                    HorizontalBox {
                        spacing: 4px;
                        height: 28px;
                        Rectangle {
                            border-radius: 4px;
                            background: FadeTheme.input-bg;
                            border-width: 1px;
                            border-color: FadeTheme.border;
                            horizontal-stretch: 1;
                            manual-input := LineEdit {
                                placeholder-text: "Add process name";
                            }
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
                }
            }
        }
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | tail -5`
Expected: Compiles successfully. The group cards scroll area is empty (just a VerticalBox with comments), which is fine — Task 4 fills it.

- [ ] **Step 3: Commit**

```bash
git add ui/main.slint
git commit -m "$(cat <<'EOF'
feat: sidebar navigation + main layout skeleton with drawer
EOF
)"
```

---

### Task 4: Group cards with app rows

This is the core of the redesign — rendering each bucket as a card with its apps listed inside.

**Files:**
- Modify: `ui/main.slint` (fill in the ScrollView inside page 0)

- [ ] **Step 1: Replace the groups ScrollView placeholder**

Find the ScrollView in page 0 that contains the comment `// GROUP CARDS GO HERE (Task 4)` and replace the entire ScrollView block (from `ScrollView {` through its closing `}`) with:

```slint
                ScrollView {
                    vertical-stretch: 1;
                    VerticalBox {
                        spacing: 12px;

                        // ── Group cards ──
                        for group[g-idx] in root.groups: Rectangle {
                            border-radius: FadeTheme.radius;
                            background: FadeTheme.card-bg;
                            border-width: 1px;
                            border-color: FadeTheme.border;

                            property <bool> expanded: true;

                            VerticalBox {
                                padding: 0px;
                                spacing: 0px;

                                // Card header
                                Rectangle {
                                    height: 44px;
                                    border-radius: FadeTheme.radius;
                                    background: FadeTheme.card-header-bg;

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
                                        // Expand/collapse toggle
                                        Rectangle {
                                            width: 24px;
                                            height: 24px;
                                            border-radius: 4px;
                                            background: exp-ta.has-hover ? FadeTheme.surface-hover : transparent;
                                            exp-ta := TouchArea {
                                                mouse-cursor: pointer;
                                                clicked => { expanded = !expanded; }
                                            }
                                            Text {
                                                text: "\u{F03EB}";
                                                font-family: "Symbols Nerd Font Mono";
                                                font-size: 14px;
                                                color: exp-ta.has-hover ? FadeTheme.accent : FadeTheme.text-dim;
                                                horizontal-alignment: center;
                                                vertical-alignment: center;
                                            }
                                        }
                                    }
                                }

                                // "+ Add app to this group" link
                                if expanded: Rectangle {
                                    height: 28px;
                                    HorizontalBox {
                                        padding-left: 16px;
                                        spacing: 4px;
                                        add-to-grp-ta := TouchArea {
                                            mouse-cursor: pointer;
                                            clicked => {
                                                root.drawer-open = true;
                                            }
                                        }
                                        Text {
                                            text: "+ Add app to this group";
                                            color: add-to-grp-ta.has-hover ? FadeTheme.accent : FadeTheme.text-dim;
                                            font-size: 11px;
                                            vertical-alignment: center;
                                        }
                                    }
                                }

                                // App rows within this group
                                if expanded: VerticalBox {
                                    padding-left: 8px;
                                    padding-right: 8px;
                                    padding-bottom: 8px;
                                    spacing: 2px;

                                    for app[a-idx] in group.apps: Rectangle {
                                        height: app.customized ? 64px : 34px;
                                        border-radius: 4px;
                                        background: app-row-ta.has-hover ? FadeTheme.surface-hover : FadeTheme.surface;
                                        app-row-ta := TouchArea {}

                                        VerticalBox {
                                            padding: 0px;
                                            spacing: 0px;

                                            // Main row — always visible
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

                                                // Status label
                                                if !app.customized: Text {
                                                    text: "Inheriting Group Settings";
                                                    color: FadeTheme.text-dim;
                                                    font-size: 11px;
                                                    font-style: italic;
                                                    vertical-alignment: center;
                                                    horizontal-stretch: 1;
                                                }
                                                if app.customized: Text {
                                                    text: "Custom Rule Applied";
                                                    color: FadeTheme.warning;
                                                    font-size: 11px;
                                                    vertical-alignment: center;
                                                    horizontal-stretch: 1;
                                                }

                                                // Customize / Reset button
                                                if !app.customized: Rectangle {
                                                    width: 130px;
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
                                                        text: "Customize Individual Rule";
                                                        font-size: 10px;
                                                        color: cust-ta.has-hover ? FadeTheme.accent : FadeTheme.text-dim;
                                                        horizontal-alignment: center;
                                                        vertical-alignment: center;
                                                    }
                                                }
                                            }

                                            // Expanded custom controls — only when customized
                                            if app.customized: HorizontalBox {
                                                height: 28px;
                                                padding-left: 52px;
                                                padding-right: 8px;
                                                spacing: 6px;

                                                Text {
                                                    text: Math.round(app-slider.value) * 5 + " min";
                                                    color: FadeTheme.text;
                                                    font-size: 11px;
                                                    width: 42px;
                                                    horizontal-alignment: right;
                                                    vertical-alignment: center;
                                                }
                                                app-slider := Slider {
                                                    width: 90px;
                                                    minimum: 1;
                                                    maximum: 24;
                                                    value: Math.ceil(app.timeout-mins / 5);
                                                    changed(val) => {
                                                        root.update-app-timeout(g-idx, a-idx, Math.round(val) * 5);
                                                    }
                                                }
                                                ComboBox {
                                                    width: 85px;
                                                    model: ["minimize", "close"];
                                                    current-value: app.action;
                                                    selected(val) => { root.update-app-action(g-idx, a-idx, val); }
                                                }
                                                Rectangle {
                                                    horizontal-stretch: 1;
                                                }
                                                Rectangle {
                                                    width: 110px;
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
                                                        text: "Reset to Group Settings";
                                                        font-size: 10px;
                                                        color: reset-ta.has-hover ? FadeTheme.bg : FadeTheme.text-dim;
                                                        horizontal-alignment: center;
                                                        vertical-alignment: center;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Unassigned Rules section ──
                        if root.unassigned-rules.length > 0: Rectangle {
                            border-radius: FadeTheme.radius;
                            background: FadeTheme.card-bg;
                            border-width: 1px;
                            border-color: FadeTheme.border;

                            VerticalBox {
                                padding: 0px;
                                spacing: 0px;

                                // Header
                                Rectangle {
                                    height: 40px;
                                    border-radius: FadeTheme.radius;
                                    background: FadeTheme.card-header-bg;
                                    HorizontalBox {
                                        padding-left: 12px;
                                        spacing: 8px;
                                        Text {
                                            text: "Special \"Individual Rules (Unassigned)\" Group";
                                            color: FadeTheme.text;
                                            font-size: 13px;
                                            font-weight: 600;
                                            vertical-alignment: center;
                                        }
                                    }
                                }

                                // Unassigned rule rows
                                VerticalBox {
                                    padding: 8px;
                                    spacing: 2px;

                                    for urule[u-idx] in root.unassigned-rules: Rectangle {
                                        height: 34px;
                                        border-radius: 4px;
                                        background: u-ta.has-hover ? FadeTheme.surface-hover : FadeTheme.surface;
                                        u-ta := TouchArea {}

                                        HorizontalBox {
                                            padding-left: 8px;
                                            padding-right: 8px;
                                            spacing: 6px;

                                            CheckBox {
                                                checked: urule.enabled;
                                                toggled => { root.toggle-unassigned(u-idx, self.checked); }
                                            }
                                            Text {
                                                text: urule.icon;
                                                font-family: "Symbols Nerd Font Mono";
                                                font-size: 13px;
                                                color: FadeTheme.accent;
                                                vertical-alignment: center;
                                                width: 18px;
                                            }
                                            Text {
                                                text: urule.process;
                                                color: FadeTheme.text;
                                                font-size: 12px;
                                                vertical-alignment: center;
                                                width: 130px;
                                                overflow: elide;
                                            }
                                            Text {
                                                text: Math.round(u-slider.value) * 5 + " min";
                                                color: FadeTheme.text;
                                                font-size: 11px;
                                                width: 42px;
                                                horizontal-alignment: right;
                                                vertical-alignment: center;
                                            }
                                            u-slider := Slider {
                                                width: 90px;
                                                minimum: 1;
                                                maximum: 24;
                                                value: Math.ceil(urule.timeout-mins / 5);
                                                changed(val) => {
                                                    root.update-unassigned-timeout(u-idx, Math.round(val) * 5);
                                                }
                                            }
                                            ComboBox {
                                                width: 85px;
                                                model: ["minimize", "close"];
                                                current-value: urule.action;
                                                selected(val) => { root.update-unassigned-action(u-idx, val); }
                                            }
                                            // Remove button
                                            Rectangle {
                                                width: 22px;
                                                height: 22px;
                                                border-radius: 4px;
                                                background: u-rm-ta.has-hover ? FadeTheme.danger : transparent;
                                                u-rm-ta := TouchArea {
                                                    mouse-cursor: pointer;
                                                    clicked => { root.remove-unassigned(u-idx); }
                                                }
                                                Text {
                                                    text: "✕";
                                                    color: u-rm-ta.has-hover ? FadeTheme.bg : FadeTheme.text-dim;
                                                    font-size: 11px;
                                                    horizontal-alignment: center;
                                                    vertical-alignment: center;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | tail -5`
Expected: Compiles successfully. The Rust side won't populate data yet (the old model types don't match), so the window will render empty groups. That's fine.

- [ ] **Step 3: Commit**

```bash
git add ui/main.slint
git commit -m "$(cat <<'EOF'
feat: group cards with app rows and unassigned rules section
EOF
)"
```

---

### Task 5: Rewrite Rust wiring — model building

The Rust side needs to build the new `GroupModel`, `UnassignedRuleModel`, and `ActiveProcessModel` types and populate them from the existing `Config`.

**Files:**
- Modify: `src/main.rs`

The key logic: for each bucket, build a `GroupModel` whose `apps` array contains one `GroupAppModel` per process in that bucket. An app is "customized" if an `app_rule` exists for that process. Unassigned rules are `app_rule` entries whose process doesn't appear in any bucket.

- [ ] **Step 1: Rewrite `update_gui_from_config` and model builders**

Replace the entire `app_rules_to_models` function and `update_gui_from_config` function (lines 184–229 of the current `src/main.rs`) with:

```rust
/// Check if a process exists in any bucket.
fn process_in_any_bucket(config: &Config, process: &str) -> bool {
    let lower = process.to_lowercase();
    config.bucket.iter().any(|b| {
        b.processes.iter().any(|p| p.to_lowercase() == lower)
    })
}

/// Find the app_rule for a process, if any.
fn find_app_rule<'a>(config: &'a Config, process: &str) -> Option<&'a config::AppRule> {
    let lower = process.to_lowercase();
    config.app_rule.iter().find(|r| r.process.to_lowercase() == lower)
}

/// Build GroupModel list from config buckets.
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
        }
    }).collect()
}

/// Build unassigned rules — app_rules whose process isn't in any bucket.
fn build_unassigned_rules(config: &Config) -> Vec<UnassignedRuleModel> {
    config.app_rule.iter()
        .filter(|r| !process_in_any_bucket(config, &r.process))
        .map(|r| UnassignedRuleModel {
            icon: icons::process_icon(&r.process).into(),
            process: r.process.clone().into(),
            timeout_mins: r.timeout_mins as i32,
            action: r.action.as_str().into(),
            enabled: r.enabled,
        })
        .collect()
}

/// Count total managed apps (enabled bucket apps + enabled unassigned rules).
fn count_managed(config: &Config) -> i32 {
    let bucket_count: usize = config.bucket.iter()
        .filter(|b| b.enabled)
        .map(|b| b.processes.iter().filter(|p| {
            // Count if no custom rule, OR if custom rule is enabled
            match find_app_rule(config, p) {
                Some(rule) => rule.enabled,
                None => true,
            }
        }).count())
        .sum();
    let unassigned_count = config.app_rule.iter()
        .filter(|r| r.enabled && !process_in_any_bucket(config, &r.process))
        .count();
    (bucket_count + unassigned_count) as i32
}

/// Populate Slint GUI properties from the Config struct.
fn update_gui_from_config(window: &SettingsWindow, config: &Config) {
    let groups = build_groups(config);
    window.set_groups(std::rc::Rc::new(slint::VecModel::from(groups)).into());

    let unassigned = build_unassigned_rules(config);
    window.set_unassigned_rules(std::rc::Rc::new(slint::VecModel::from(unassigned)).into());

    window.set_managed_count(count_managed(config));
    window.set_polling_interval_secs(config.general.polling_interval_secs as i32);
    window.set_auto_start(config.general.auto_start);
    window.set_version(env!("CARGO_PKG_VERSION").into());
}
```

- [ ] **Step 2: Rewrite `refresh_active_windows` to build `ActiveProcessModel`**

Replace the existing `refresh_active_windows` function with:

```rust
/// Refresh the active processes in the drawer + active count.
fn refresh_active_windows(
    window: &SettingsWindow,
    config: &Config,
    snapshot_buffer: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
) {
    if let Ok(buf) = snapshot_buffer.lock() {
        // Deduplicate by process name (keep first occurrence)
        let mut seen = std::collections::HashSet::new();
        let models: Vec<ActiveProcessModel> = buf
            .iter()
            .filter(|s| !config.is_hidden(&s.process))
            .filter(|s| seen.insert(s.process.to_lowercase()))
            .map(|s| ActiveProcessModel {
                icon: icons::process_icon(&s.process).into(),
                process: s.process.clone().into(),
                managed: config.resolve_process(&s.process).is_some(),
            })
            .collect();
        window.set_active_count(models.len() as i32);
        window.set_active_processes(
            std::rc::Rc::new(slint::VecModel::from(models)).into(),
        );
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | tail -10`
Expected: Will likely fail because `setup_gui_callbacks` still references old callbacks. That's fine — Task 6 fixes it. Confirm the model building functions compile without type errors. Errors should only be about missing/renamed callbacks.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
feat: build group/unassigned/active models from config
EOF
)"
```

---

### Task 6: Rewrite Rust wiring — callbacks

Replace `setup_gui_callbacks` with handlers for the new callback set.

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `setup_gui_callbacks`**

Replace the entire `setup_gui_callbacks` function with:

```rust
/// Wire Slint callbacks to modify the shared config.
fn setup_gui_callbacks(
    window: &SettingsWindow,
    config: Arc<RwLock<Config>>,
    snapshot_buffer: Arc<Mutex<Vec<ActiveWindowSnapshot>>>,
) {
    // Helper: full refresh after any mutation
    let refresh_all = {
        move |cfg: &Config,
              weak: &slint::Weak<SettingsWindow>,
              snap: &Arc<Mutex<Vec<ActiveWindowSnapshot>>>| {
            if let Some(w) = weak.upgrade() {
                update_gui_from_config(&w, cfg);
                refresh_active_windows(&w, cfg, snap);
            }
        }
    };

    // ── Group (bucket) callbacks ──

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_toggle_group(move |idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].enabled = enabled;
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    let cfg = config.clone();
    window.on_update_group_timeout(move |idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].timeout_mins = mins as u64;
                let _ = c.save();
            }
        }
    });

    let cfg = config.clone();
    window.on_update_group_action(move |idx, action| {
        if let Ok(mut c) = cfg.write() {
            let idx = idx as usize;
            if idx < c.bucket.len() {
                c.bucket[idx].action = Action::from_str(&action);
                let _ = c.save();
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_add_app_to_group(move |g_idx, process| {
        let process_str = process.to_string();
        if process_str.is_empty() { return; }
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            if g < c.bucket.len() {
                let already = c.bucket[g].processes.iter().any(|p| p.eq_ignore_ascii_case(&process_str));
                if !already {
                    c.bucket[g].processes.push(process_str);
                    let _ = c.save();
                    refresh_all(&c, &weak, &snap);
                }
            }
        }
    });

    // ── App-in-group callbacks ──

    // Customize: create an app_rule for a bucket process (copies bucket settings as starting point)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_customize_app(move |g_idx, a_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process = c.bucket[g].processes[a].clone();
                // Only create if no app_rule exists yet
                let exists = c.app_rule.iter().any(|r| r.process.eq_ignore_ascii_case(&process));
                if !exists {
                    c.app_rule.push(config::AppRule {
                        process,
                        timeout_mins: c.bucket[g].timeout_mins,
                        action: c.bucket[g].action.clone(),
                        enabled: true,
                    });
                    let _ = c.save();
                    refresh_all(&c, &weak, &snap);
                }
            }
        }
    });

    // Reset to group: delete the app_rule for this bucket process
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_reset_app_to_group(move |g_idx, a_idx| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process_lower = c.bucket[g].processes[a].to_lowercase();
                c.app_rule.retain(|r| r.process.to_lowercase() != process_lower);
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    let cfg = config.clone();
    window.on_update_app_timeout(move |g_idx, a_idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process_lower = c.bucket[g].processes[a].to_lowercase();
                if let Some(rule) = c.app_rule.iter_mut().find(|r| r.process.to_lowercase() == process_lower) {
                    rule.timeout_mins = mins as u64;
                    let _ = c.save();
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_app_action(move |g_idx, a_idx, action| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process_lower = c.bucket[g].processes[a].to_lowercase();
                if let Some(rule) = c.app_rule.iter_mut().find(|r| r.process.to_lowercase() == process_lower) {
                    rule.action = Action::from_str(&action);
                    let _ = c.save();
                }
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_toggle_app(move |g_idx, a_idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let g = g_idx as usize;
            let a = a_idx as usize;
            if g < c.bucket.len() && a < c.bucket[g].processes.len() {
                let process = &c.bucket[g].processes[a];
                let process_lower = process.to_lowercase();
                // If toggling an inherited app, create a custom rule first
                let exists = c.app_rule.iter().any(|r| r.process.to_lowercase() == process_lower);
                if !exists {
                    c.app_rule.push(config::AppRule {
                        process: process.clone(),
                        timeout_mins: c.bucket[g].timeout_mins,
                        action: c.bucket[g].action.clone(),
                        enabled,
                    });
                } else if let Some(rule) = c.app_rule.iter_mut().find(|r| r.process.to_lowercase() == process_lower) {
                    rule.enabled = enabled;
                }
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    // ── Unassigned rule callbacks ──

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_remove_unassigned(move |idx| {
        if let Ok(mut c) = cfg.write() {
            // Find the idx-th unassigned rule
            let unassigned_processes: Vec<String> = c.app_rule.iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                let proc = proc.clone();
                c.app_rule.retain(|r| r.process.to_lowercase() != proc);
                let _ = c.save();
                refresh_all(&c, &weak, &snap);
            }
        }
    });

    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_toggle_unassigned(move |idx, enabled| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c.app_rule.iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                if let Some(rule) = c.app_rule.iter_mut().find(|r| r.process.to_lowercase() == *proc) {
                    rule.enabled = enabled;
                    let _ = c.save();
                    refresh_all(&c, &weak, &snap);
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_unassigned_timeout(move |idx, mins| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c.app_rule.iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                if let Some(rule) = c.app_rule.iter_mut().find(|r| r.process.to_lowercase() == *proc) {
                    rule.timeout_mins = mins as u64;
                    let _ = c.save();
                }
            }
        }
    });

    let cfg = config.clone();
    window.on_update_unassigned_action(move |idx, action| {
        if let Ok(mut c) = cfg.write() {
            let unassigned_processes: Vec<String> = c.app_rule.iter()
                .filter(|r| !process_in_any_bucket(&c, &r.process))
                .map(|r| r.process.to_lowercase())
                .collect();
            if let Some(proc) = unassigned_processes.get(idx as usize) {
                if let Some(rule) = c.app_rule.iter_mut().find(|r| r.process.to_lowercase() == *proc) {
                    rule.action = Action::from_str(&action);
                    let _ = c.save();
                }
            }
        }
    });

    // ── Drawer callbacks ──

    // add-rule: add as unassigned app_rule (from active process drawer)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_add_rule(move |process| {
        let process_str = process.to_string();
        if process_str.is_empty() { return; }
        if let Ok(mut c) = cfg.write() {
            if c.app_rule.iter().any(|r| r.process.eq_ignore_ascii_case(&process_str)) {
                return;
            }
            // Also skip if already in a bucket
            if process_in_any_bucket(&c, &process_str) {
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

    // add-process-name: same as add-rule (manual text entry)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_add_process_name(move |process| {
        let process_str = process.to_string();
        if process_str.is_empty() { return; }
        if let Ok(mut c) = cfg.write() {
            if c.app_rule.iter().any(|r| r.process.eq_ignore_ascii_case(&process_str)) {
                return;
            }
            if process_in_any_bucket(&c, &process_str) {
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

    // ── General settings ──

    let cfg = config.clone();
    window.on_set_polling_interval(move |secs| {
        if let Ok(mut c) = cfg.write() {
            c.general.polling_interval_secs = secs as u64;
            let _ = c.save();
        }
    });

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

    // hide-process (kept for potential future use)
    let cfg = config.clone();
    let weak = window.as_weak();
    let snap = snapshot_buffer.clone();
    window.on_hide_process(move |process| {
        if let Ok(mut c) = cfg.write() {
            let process_str = process.to_string();
            if !c.general.hidden_processes.contains(&process_str) {
                c.general.hidden_processes.push(process_str);
                let _ = c.save();
                if let Some(w) = weak.upgrade() {
                    refresh_active_windows(&w, &c, &snap);
                }
            }
        }
    });
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | tail -10`
Expected: Compiles successfully.

- [ ] **Step 3: Run tests**

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass. The config tests are independent of the UI layer.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
feat: rewrite all GUI callbacks for unified group/app model
EOF
)"
```

---

### Task 7: Clean up dead code and verify full build

Remove old model types and unused code that the Slint compiler no longer generates.

**Files:**
- Modify: `src/main.rs` (remove any leftover dead code)

- [ ] **Step 1: Remove old imports if any remain**

Check `src/main.rs` for any references to old types like `AppRuleModel`, `BucketModel`, `ActiveWindowModel` (the old Slint-generated types). The Slint compiler now generates `GroupModel`, `GroupAppModel`, `UnassignedRuleModel`, and `ActiveProcessModel` instead. Remove any lingering references to the old types.

- [ ] **Step 2: Full build + test**

Run: `cargo build --target x86_64-pc-windows-gnu 2>&1 | tail -5`
Expected: Clean compile.

Run: `cargo test 2>&1 | tail -10`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore: remove dead code from old three-tab UI
EOF
)"
```

---

### Task 8: Polish and visual refinements

Final pass to match the mockup more closely — adjust spacing, colors, and any visual issues discovered during testing.

**Files:**
- Modify: `ui/main.slint` (spacing/sizing tweaks)
- Modify: `ui/style.slint` (color adjustments if needed)

- [ ] **Step 1: Build release binary for visual testing**

Run: `cargo build --release --target x86_64-pc-windows-gnu 2>&1 | tail -5`
Expected: Clean compile.

- [ ] **Step 2: Run the binary on Windows and compare against mockup**

Transfer `target/x86_64-pc-windows-gnu/release/fade.exe` to Windows and run it. Compare the layout against the mockup screenshot. Note any spacing, font size, or alignment issues.

- [ ] **Step 3: Fix any visual issues found**

Apply fixes to `ui/main.slint` and/or `ui/style.slint` as needed. Rebuild and re-test.

- [ ] **Step 4: Final commit**

```bash
git add ui/main.slint ui/style.slint
git commit -m "$(cat <<'EOF'
fix: visual polish for unified UI layout
EOF
)"
```

---

## Self-Review

**Spec coverage:**
- Sidebar navigation (Applications & Rules / General Settings) → Task 3
- Group cards with global settings → Task 4
- App rows with Inheriting/Customized status → Task 4
- Custom rule expand with sliders → Task 4
- Reset to Bucket button → Task 4 (UI) + Task 6 (callback)
- Unassigned rules section → Task 4
- Active Processes drawer → Task 3
- Manual process name entry → Task 3
- Search box → Task 3 (UI rendered, filtering is visual-only via Slint — future enhancement)
- Managed count / active count in header → Task 3 (UI) + Task 5 (Rust)
- "+ Add New Managed App" button → Task 3

**Placeholder scan:** No TBDs, TODOs, or "similar to Task N" references found.

**Type consistency check:**
- `GroupModel` used in Tasks 2, 4, 5: consistent fields (`icon`, `name`, `enabled`, `timeout-mins`, `action`, `apps`)
- `GroupAppModel` used in Tasks 2, 4, 5: consistent fields (`icon`, `process`, `customized`, `enabled`, `timeout-mins`, `action`)
- `UnassignedRuleModel` used in Tasks 2, 4, 5: consistent fields (`icon`, `process`, `timeout-mins`, `action`, `enabled`)
- `ActiveProcessModel` used in Tasks 2, 3, 5: consistent fields (`icon`, `process`, `managed`)
- Callback names match between Task 2 (declarations) and Task 6 (handlers)

**Note on search:** The search box is rendered in the UI but not wired to filter groups/apps. This is intentional — it's a visual placeholder matching the mockup. Filtering can be added later as an enhancement without changing the data model. If the user wants it wired up now, it would require passing `search-text` to Rust and filtering the models server-side, since Slint `for` loops can't conditionally skip items based on a property.
