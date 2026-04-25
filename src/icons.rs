/// Returns either a brand slug (rendered as an SVG by the BrandIcon Slint
/// component) or a Nerd Font glyph (single Unicode character) for unknown apps.
pub fn process_icon(process: &str) -> &'static str {
    match process.to_lowercase().as_str() {
        // Browsers
        "chrome.exe" => "googlechrome",
        "firefox.exe" => "firefox",
        "msedge.exe" => "\u{F01E9}",
        "brave.exe" => "brave",
        "opera.exe" => "opera",
        "vivaldi.exe" => "vivaldi",
        "arc.exe" => "arc",
        // Communication
        "slack.exe" => "slack",
        "discord.exe" => "discord",
        "teams.exe" => "\u{F02BB}",
        "telegram.exe" => "telegram",
        "signal.exe" => "signal",
        "whatsapp.exe" => "whatsapp",
        // Media
        "spotify.exe" => "spotify",
        "vlc.exe" => "vlcmediaplayer",
        "itunes.exe" => "itunes",
        "foobar2000.exe" => "\u{F075A}",
        // Development
        "code.exe" => "visualstudiocode",
        "idea64.exe" => "intellijidea",
        "studio64.exe" => "androidstudio",
        "devenv.exe" => "visualstudio",
        // Gaming
        "steam.exe" => "steam",
        "epicgameslauncher.exe" => "epicgames",
        "galaxyclient.exe" => "gogdotcom",
        // System / Utilities
        "explorer.exe" => "\u{F024B}",
        "notepad.exe" | "notepad++.exe" => "\u{F0219}",
        "rustdesk.exe" => "\u{F0379}",
        "powershell.exe" | "pwsh.exe" => "powershell",
        "cmd.exe" => "\u{F018D}",
        "windowsterminal.exe" => "windowsterminal",
        "mstsc.exe" => "\u{F0379}",
        "taskmgr.exe" => "\u{F035C}",
        "winword.exe" => "\u{F021B}",
        "excel.exe" => "\u{F021F}",
        "powerpnt.exe" => "\u{F0227}",
        "outlook.exe" => "\u{F0D22}",
        "claude.exe" => "anthropic",
        "codex.exe" => "openai",
        "antigravity.exe" => "googlegemini",
        "cursor.exe" => "cursor",
        // Fallback
        _ => "\u{F0485}",
    }
}

/// Icon for a bucket category name (always glyph — categories don't have brands).
pub fn bucket_icon(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "browsing" => "\u{F0483}",
        "communication" => "\u{F0BE8}",
        "media" => "\u{F04C5}",
        "development" => "\u{F0A1E}",
        "gaming" => "\u{F0E52}",
        _ => "\u{F0493}",
    }
}
