/// Maps process names to Nerd Font (Material Design Icons) glyphs.

pub fn process_icon(process: &str) -> &'static str {
    match process.to_lowercase().as_str() {
        // Browsers
        "chrome.exe" => "\u{F0E28}",
        "firefox.exe" => "\u{F0239}",
        "msedge.exe" => "\u{F01E9}",
        "brave.exe" => "\u{F0E5D}",
        "opera.exe" => "\u{F0E43}",
        "vivaldi.exe" => "\u{F0483}",
        "arc.exe" => "\u{F0483}",
        // Communication
        "slack.exe" => "\u{F04B1}",
        "discord.exe" => "\u{F066F}",
        "teams.exe" => "\u{F02BB}",
        "telegram.exe" => "\u{F0443}",
        "signal.exe" => "\u{F1190}",
        "whatsapp.exe" => "\u{F05A0}",
        // Media
        "spotify.exe" => "\u{F04C7}",
        "vlc.exe" => "\u{F057C}",
        "itunes.exe" => "\u{F04C5}",
        "foobar2000.exe" => "\u{F075A}",
        // Development
        "code.exe" => "\u{F0A1E}",
        "idea64.exe" => "\u{F04CC}",
        "studio64.exe" => "\u{F04CC}",
        "devenv.exe" => "\u{F0610}",
        // Gaming
        "steam.exe" => "\u{F04A5}",
        "epicgameslauncher.exe" => "\u{F0E52}",
        "galaxyclient.exe" => "\u{F0E52}",
        // System / Utilities
        "explorer.exe" => "\u{F024B}", // nf-md-folder
        "notepad.exe" | "notepad++.exe" => "\u{F0219}", // nf-md-file-document
        "rustdesk.exe" => "\u{F0379}", // nf-md-monitor
        "powershell.exe" | "pwsh.exe" => "\u{F018D}", // nf-md-console
        "cmd.exe" => "\u{F018D}",
        "windowsterminal.exe" => "\u{F018D}",
        "mstsc.exe" => "\u{F0379}", // nf-md-monitor (remote desktop)
        "taskmgr.exe" => "\u{F035C}", // nf-md-memory
        "winword.exe" => "\u{F021B}", // nf-md-file-word
        "excel.exe" => "\u{F021F}", // nf-md-file-excel
        "powerpnt.exe" => "\u{F0227}", // nf-md-file-powerpoint
        "outlook.exe" => "\u{F0D22}", // nf-md-email
        "codex.exe" => "\u{F018D}", // nf-md-console
        "antigravity.exe" => "\u{F0E52}", // nf-md-gamepad-variant
        // Fallback
        _ => "\u{F0485}", // nf-md-window-maximize
    }
}

/// Icon for a bucket category name.
pub fn bucket_icon(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "browsing" => "\u{F0483}",    // nf-md-web
        "communication" => "\u{F0BE8}", // nf-md-chat
        "media" => "\u{F04C5}",       // nf-md-music
        "development" => "\u{F0A1E}", // nf-md-microsoft_visual_studio_code
        "gaming" => "\u{F04A5}",      // nf-md-steam (gamepad would be F0E52)
        _ => "\u{F0493}",             // nf-md-widgets
    }
}
