/// Maps process names to Nerd Font (Material Design Icons) glyphs.

pub fn process_icon(process: &str) -> &'static str {
    match process.to_lowercase().as_str() {
        // Browsers
        "chrome.exe" => "\u{F268}",
        "firefox.exe" => "\u{F269}",
        "msedge.exe" => "\u{F01E9}",
        "brave.exe" => "\u{F0E5D}",
        "opera.exe" => "\u{F0E43}",
        "vivaldi.exe" => "\u{F0483}",
        "arc.exe" => "\u{F08C7}",
        // Communication
        "slack.exe" => "\u{F04B1}",
        "discord.exe" => "\u{F066F}",
        "teams.exe" => "\u{F02BB}",
        "telegram.exe" => "\u{E217}",
        "signal.exe" => "\u{F1190}",
        "whatsapp.exe" => "\u{F05A3}",
        // Media
        "spotify.exe" => "\u{F04C7}",
        "vlc.exe" => "\u{F057C}",
        "itunes.exe" => "\u{F2E9}",
        "foobar2000.exe" => "\u{F075A}",
        // Development
        "code.exe" => "\u{F0A1E}",
        "idea64.exe" => "\u{F04CC}",
        "studio64.exe" => "\u{F04CC}",
        "devenv.exe" => "\u{F0610}",
        // Gaming
        "steam.exe" => "\u{F1B6}",
        "epicgameslauncher.exe" => "\u{F0E52}",
        "galaxyclient.exe" => "\u{E243}",
        // System / Utilities
        "explorer.exe" => "\u{F024B}",
        "notepad.exe" | "notepad++.exe" => "\u{F0219}",
        "rustdesk.exe" => "\u{F0379}",
        "powershell.exe" | "pwsh.exe" => "\u{F018D}",
        "cmd.exe" => "\u{F018D}",
        "windowsterminal.exe" => "\u{F018D}",
        "mstsc.exe" => "\u{F0379}",
        "taskmgr.exe" => "\u{F035C}",
        "winword.exe" => "\u{F021B}",
        "excel.exe" => "\u{F021F}",
        "powerpnt.exe" => "\u{F0227}",
        "outlook.exe" => "\u{F0D22}",
        "codex.exe" => "\u{F018D}",
        "antigravity.exe" => "\u{F0E52}",
        // Fallback
        _ => "\u{F0485}",
    }
}

/// Icon for a bucket category name.
pub fn bucket_icon(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "browsing" => "\u{F0483}",      // nf-md-web
        "communication" => "\u{F0BE8}", // nf-md-chat
        "media" => "\u{F04C5}",         // nf-md-music
        "development" => "\u{F0A1E}",   // nf-md-microsoft_visual_studio_code
        "gaming" => "\u{F0E52}",        // nf-md-gamepad-variant
        _ => "\u{F0493}",               // nf-md-widgets
    }
}
