/// Searchable catalog of Nerd Font Material Design Icons.
///
/// Each entry pairs a glyph codepoint with a space-separated keyword string.
/// `search(query)` matches keywords by case-insensitive substring.

pub struct IconEntry {
    pub glyph: &'static str,
    pub keywords: &'static str,
}

/// Curated subset of Nerd Font MDI glyphs chosen to cover common application categories.
/// Keyword strings include synonyms so search is forgiving.
pub const CATALOG: &[IconEntry] = &[
    // Browsers / web
    IconEntry { glyph: "\u{F02AD}", keywords: "browser chrome google web" },
    IconEntry { glyph: "\u{F0239}", keywords: "browser firefox web fox" },
    IconEntry { glyph: "\u{F01E9}", keywords: "browser edge microsoft web" },
    IconEntry { glyph: "\u{F0E5D}", keywords: "browser brave lion web" },
    IconEntry { glyph: "\u{F0E43}", keywords: "browser opera web" },
    IconEntry { glyph: "\u{F0483}", keywords: "web globe internet network www" },
    IconEntry { glyph: "\u{F0D2A}", keywords: "web world earth globe" },
    IconEntry { glyph: "\u{F0FBC}", keywords: "search find magnify" },
    IconEntry { glyph: "\u{F0349}", keywords: "search magnify find" },
    IconEntry { glyph: "\u{F0C7F}", keywords: "bookmark ribbon save" },
    IconEntry { glyph: "\u{F00C0}", keywords: "bookmark outline" },
    IconEntry { glyph: "\u{F0A38}", keywords: "link url chain" },
    IconEntry { glyph: "\u{F0337}", keywords: "lock secure https padlock" },
    IconEntry { glyph: "\u{F033F}", keywords: "unlock insecure open padlock" },
    IconEntry { glyph: "\u{F0E7A}", keywords: "shield protection security" },
    IconEntry { glyph: "\u{F0498}", keywords: "shield check security" },

    // Communication / chat / mail / social
    IconEntry { glyph: "\u{F04B1}", keywords: "slack chat message" },
    IconEntry { glyph: "\u{F066F}", keywords: "discord chat game" },
    IconEntry { glyph: "\u{F02BB}", keywords: "microsoft teams chat" },
    IconEntry { glyph: "\u{F0443}", keywords: "telegram chat message paper" },
    IconEntry { glyph: "\u{F1190}", keywords: "signal chat secure message" },
    IconEntry { glyph: "\u{F05A0}", keywords: "whatsapp chat message" },
    IconEntry { glyph: "\u{F0BE8}", keywords: "chat bubble message" },
    IconEntry { glyph: "\u{F0361}", keywords: "message chat" },
    IconEntry { glyph: "\u{F02FC}", keywords: "message text bubble" },
    IconEntry { glyph: "\u{F0D22}", keywords: "email mail envelope outlook" },
    IconEntry { glyph: "\u{F01EE}", keywords: "email mail envelope" },
    IconEntry { glyph: "\u{F0CFC}", keywords: "email send inbox" },
    IconEntry { glyph: "\u{F048C}", keywords: "telephone phone call" },
    IconEntry { glyph: "\u{F03F2}", keywords: "phone mobile cell" },
    IconEntry { glyph: "\u{F016F}", keywords: "video camera call meeting" },
    IconEntry { glyph: "\u{F0567}", keywords: "microphone mic audio voice" },
    IconEntry { glyph: "\u{F036D}", keywords: "microphone off mute" },
    IconEntry { glyph: "\u{F0581}", keywords: "headphones audio headset" },
    IconEntry { glyph: "\u{F004F}", keywords: "bell notification alert" },
    IconEntry { glyph: "\u{F009A}", keywords: "bell off silent mute" },
    IconEntry { glyph: "\u{F00EB}", keywords: "calendar date schedule" },
    IconEntry { glyph: "\u{F0ED8}", keywords: "calendar today date" },
    IconEntry { glyph: "\u{F05CC}", keywords: "contacts people users" },
    IconEntry { glyph: "\u{F0013}", keywords: "account user profile person" },
    IconEntry { glyph: "\u{F0850}", keywords: "account group team users" },
    IconEntry { glyph: "\u{F0014}", keywords: "account circle user avatar" },

    // Media / music / video / photo
    IconEntry { glyph: "\u{F04C7}", keywords: "spotify music audio" },
    IconEntry { glyph: "\u{F057C}", keywords: "vlc media player cone" },
    IconEntry { glyph: "\u{F04C5}", keywords: "music note audio song" },
    IconEntry { glyph: "\u{F075A}", keywords: "music speaker audio" },
    IconEntry { glyph: "\u{F0388}", keywords: "music queue audio" },
    IconEntry { glyph: "\u{F040A}", keywords: "play media video triangle" },
    IconEntry { glyph: "\u{F03E4}", keywords: "pause media" },
    IconEntry { glyph: "\u{F04DB}", keywords: "stop media square" },
    IconEntry { glyph: "\u{F04AD}", keywords: "skip next forward media" },
    IconEntry { glyph: "\u{F04AE}", keywords: "skip previous back media" },
    IconEntry { glyph: "\u{F0381}", keywords: "volume high audio speaker" },
    IconEntry { glyph: "\u{F0583}", keywords: "volume off mute audio" },
    IconEntry { glyph: "\u{F0567}", keywords: "radio broadcast tuner" },
    IconEntry { glyph: "\u{F0568}", keywords: "radio tower antenna signal" },
    IconEntry { glyph: "\u{F0567}", keywords: "podcast audio show" },
    IconEntry { glyph: "\u{F05A5}", keywords: "video movie film" },
    IconEntry { glyph: "\u{F00F2}", keywords: "camera photo picture" },
    IconEntry { glyph: "\u{F02E9}", keywords: "image picture photo" },
    IconEntry { glyph: "\u{F08FD}", keywords: "image multiple gallery photos" },
    IconEntry { glyph: "\u{F0579}", keywords: "television tv screen display" },
    IconEntry { glyph: "\u{F024B}", keywords: "folder directory file" },
    IconEntry { glyph: "\u{F0256}", keywords: "folder music audio" },
    IconEntry { glyph: "\u{F024E}", keywords: "folder image photo picture" },
    IconEntry { glyph: "\u{F0251}", keywords: "folder video movie" },
    IconEntry { glyph: "\u{F0770}", keywords: "folder account user home" },
    IconEntry { glyph: "\u{F0257}", keywords: "folder open" },

    // Development / code
    IconEntry { glyph: "\u{F0A1E}", keywords: "vscode code editor microsoft development" },
    IconEntry { glyph: "\u{F04CC}", keywords: "jetbrains android studio development ide" },
    IconEntry { glyph: "\u{F0610}", keywords: "visual studio development ide microsoft" },
    IconEntry { glyph: "\u{F0624}", keywords: "code tags development programming" },
    IconEntry { glyph: "\u{F0175}", keywords: "code braces development json" },
    IconEntry { glyph: "\u{F014F}", keywords: "code slash development" },
    IconEntry { glyph: "\u{F01C3}", keywords: "console terminal command shell" },
    IconEntry { glyph: "\u{F018D}", keywords: "console terminal powershell cmd shell" },
    IconEntry { glyph: "\u{F0B5E}", keywords: "bash terminal shell linux" },
    IconEntry { glyph: "\u{F02A0}", keywords: "git version control" },
    IconEntry { glyph: "\u{F02A4}", keywords: "git branch merge version" },
    IconEntry { glyph: "\u{F02A8}", keywords: "github octocat" },
    IconEntry { glyph: "\u{F02A6}", keywords: "gitlab" },
    IconEntry { glyph: "\u{F0F9C}", keywords: "database db storage data" },
    IconEntry { glyph: "\u{F01BC}", keywords: "bug debug insect issue" },
    IconEntry { glyph: "\u{F0340}", keywords: "wrench tool settings fix" },
    IconEntry { glyph: "\u{F0493}", keywords: "widgets boxes modules" },
    IconEntry { glyph: "\u{F08FF}", keywords: "api cloud integration" },
    IconEntry { glyph: "\u{F012F}", keywords: "cloud upload backup" },
    IconEntry { glyph: "\u{F01F2}", keywords: "cloud download" },
    IconEntry { glyph: "\u{F015F}", keywords: "cloud storage sync" },
    IconEntry { glyph: "\u{F06FC}", keywords: "docker container whale" },
    IconEntry { glyph: "\u{F0FD2}", keywords: "kubernetes k8s container orchestration" },
    IconEntry { glyph: "\u{F0F81}", keywords: "language python snake programming" },
    IconEntry { glyph: "\u{F031E}", keywords: "language javascript js programming" },
    IconEntry { glyph: "\u{F0626}", keywords: "language rust crab programming" },
    IconEntry { glyph: "\u{F0A38}", keywords: "language go programming" },
    IconEntry { glyph: "\u{F0772}", keywords: "language c programming" },
    IconEntry { glyph: "\u{F0674}", keywords: "language java coffee programming" },
    IconEntry { glyph: "\u{F031F}", keywords: "language html web markup" },
    IconEntry { glyph: "\u{F031C}", keywords: "language css style web" },

    // Gaming / controller / dice
    IconEntry { glyph: "\u{F04A5}", keywords: "steam game valve" },
    IconEntry { glyph: "\u{F0E52}", keywords: "gamepad variant controller console" },
    IconEntry { glyph: "\u{F0297}", keywords: "gamepad controller playstation" },
    IconEntry { glyph: "\u{F06F8}", keywords: "xbox controller console" },
    IconEntry { glyph: "\u{F0E51}", keywords: "gamepad classic nes retro" },
    IconEntry { glyph: "\u{F0207}", keywords: "puzzle jigsaw game" },
    IconEntry { glyph: "\u{F01CA}", keywords: "chess king game strategy" },
    IconEntry { glyph: "\u{F01F7}", keywords: "dice game random gamble" },
    IconEntry { glyph: "\u{F0D9A}", keywords: "cards playing game spade" },
    IconEntry { glyph: "\u{F01F9}", keywords: "dice 6 six random" },
    IconEntry { glyph: "\u{F0175}", keywords: "trophy award achievement" },

    // Office / productivity / documents
    IconEntry { glyph: "\u{F021B}", keywords: "microsoft word document office" },
    IconEntry { glyph: "\u{F021F}", keywords: "microsoft excel spreadsheet office" },
    IconEntry { glyph: "\u{F0227}", keywords: "microsoft powerpoint slides office" },
    IconEntry { glyph: "\u{F0219}", keywords: "file document page" },
    IconEntry { glyph: "\u{F021A}", keywords: "file document box" },
    IconEntry { glyph: "\u{F0214}", keywords: "file pdf document" },
    IconEntry { glyph: "\u{F0220}", keywords: "file excel spreadsheet xlsx" },
    IconEntry { glyph: "\u{F022A}", keywords: "file word docx document" },
    IconEntry { glyph: "\u{F0226}", keywords: "file powerpoint pptx slides" },
    IconEntry { glyph: "\u{F0228}", keywords: "file image photo picture" },
    IconEntry { glyph: "\u{F022C}", keywords: "file zip archive compressed" },
    IconEntry { glyph: "\u{F0224}", keywords: "file music audio" },
    IconEntry { glyph: "\u{F0225}", keywords: "file video movie" },
    IconEntry { glyph: "\u{F0FB6}", keywords: "file code source development" },
    IconEntry { glyph: "\u{F0AFB}", keywords: "file multiple stack papers" },
    IconEntry { glyph: "\u{F0214}", keywords: "notebook writing journal" },
    IconEntry { glyph: "\u{F00E1}", keywords: "book read library" },
    IconEntry { glyph: "\u{F02E2}", keywords: "book open read" },
    IconEntry { glyph: "\u{F0117}", keywords: "checkbox list tasks todo" },
    IconEntry { glyph: "\u{F011A}", keywords: "checkbox marked done todo" },
    IconEntry { glyph: "\u{F0139}", keywords: "clipboard text notes" },
    IconEntry { glyph: "\u{F0513}", keywords: "clipboard check done tasks" },
    IconEntry { glyph: "\u{F021C}", keywords: "file edit pen document" },
    IconEntry { glyph: "\u{F0150}", keywords: "clock time watch" },
    IconEntry { glyph: "\u{F020C}", keywords: "clock outline time" },
    IconEntry { glyph: "\u{F00B8}", keywords: "calculator math numbers" },
    IconEntry { glyph: "\u{F0131}", keywords: "chart bar graph stats" },
    IconEntry { glyph: "\u{F0682}", keywords: "chart pie stats" },
    IconEntry { glyph: "\u{F0129}", keywords: "chart line graph stats" },
    IconEntry { glyph: "\u{F12C1}", keywords: "chart timeline stats history" },
    IconEntry { glyph: "\u{F00EB}", keywords: "calendar planner schedule" },
    IconEntry { glyph: "\u{F0F8B}", keywords: "presentation slides meeting" },
    IconEntry { glyph: "\u{F0FA5}", keywords: "briefcase work business office" },
    IconEntry { glyph: "\u{F0333}", keywords: "briefcase outline work" },
    IconEntry { glyph: "\u{F0CC0}", keywords: "palette paint design art color" },
    IconEntry { glyph: "\u{F0B8E}", keywords: "brush paint design art" },
    IconEntry { glyph: "\u{F02B6}", keywords: "pencil edit write draw" },
    IconEntry { glyph: "\u{F03EB}", keywords: "pencil outline edit" },
    IconEntry { glyph: "\u{F03EA}", keywords: "pencil box edit" },
    IconEntry { glyph: "\u{F1B09}", keywords: "pen ink write" },

    // System / utilities / hardware / settings
    IconEntry { glyph: "\u{F0493}", keywords: "apps grid widgets" },
    IconEntry { glyph: "\u{F0493}", keywords: "dots grid apps" },
    IconEntry { glyph: "\u{F0493}", keywords: "view dashboard grid" },
    IconEntry { glyph: "\u{F0485}", keywords: "window maximize fullscreen" },
    IconEntry { glyph: "\u{F0395}", keywords: "window minimize" },
    IconEntry { glyph: "\u{F0492}", keywords: "window close" },
    IconEntry { glyph: "\u{F0379}", keywords: "monitor display screen" },
    IconEntry { glyph: "\u{F06A9}", keywords: "monitor multiple dual" },
    IconEntry { glyph: "\u{F01C8}", keywords: "laptop notebook computer" },
    IconEntry { glyph: "\u{F0379}", keywords: "desktop tower computer pc" },
    IconEntry { glyph: "\u{F0322}", keywords: "keyboard input typing" },
    IconEntry { glyph: "\u{F037D}", keywords: "mouse pointer input" },
    IconEntry { glyph: "\u{F035C}", keywords: "memory chip ram" },
    IconEntry { glyph: "\u{F061A}", keywords: "chip cpu processor" },
    IconEntry { glyph: "\u{F02CA}", keywords: "harddisk drive storage ssd" },
    IconEntry { glyph: "\u{F02C9}", keywords: "harddisk plus backup storage" },
    IconEntry { glyph: "\u{F0211}", keywords: "battery charge power" },
    IconEntry { glyph: "\u{F0206}", keywords: "battery full power" },
    IconEntry { glyph: "\u{F00D8}", keywords: "power plug charger electric" },
    IconEntry { glyph: "\u{F0425}", keywords: "power on off toggle" },
    IconEntry { glyph: "\u{F0A42}", keywords: "cog settings gear options" },
    IconEntry { glyph: "\u{F0493}", keywords: "settings preferences" },
    IconEntry { glyph: "\u{F0A4A}", keywords: "cog outline settings" },
    IconEntry { glyph: "\u{F0493}", keywords: "tune slider options equalizer" },
    IconEntry { glyph: "\u{F02FE}", keywords: "filter sort" },
    IconEntry { glyph: "\u{F0238}", keywords: "wifi wireless network signal" },
    IconEntry { glyph: "\u{F099C}", keywords: "wifi off disabled" },
    IconEntry { glyph: "\u{F0092}", keywords: "bluetooth wireless" },
    IconEntry { glyph: "\u{F0098}", keywords: "bluetooth off" },
    IconEntry { glyph: "\u{F0200}", keywords: "lan network cable" },
    IconEntry { glyph: "\u{F02DB}", keywords: "printer print" },
    IconEntry { glyph: "\u{F04E6}", keywords: "usb drive flash storage" },
    IconEntry { glyph: "\u{F0D13}", keywords: "eject remove" },
    IconEntry { glyph: "\u{F0450}", keywords: "trash delete remove bin" },
    IconEntry { glyph: "\u{F05E8}", keywords: "trash empty delete" },
    IconEntry { glyph: "\u{F01A2}", keywords: "archive box store" },
    IconEntry { glyph: "\u{F06D1}", keywords: "export upload out" },
    IconEntry { glyph: "\u{F06C5}", keywords: "import download in" },
    IconEntry { glyph: "\u{F0162}", keywords: "content save disk" },
    IconEntry { glyph: "\u{F0157}", keywords: "content copy duplicate" },
    IconEntry { glyph: "\u{F0192}", keywords: "content cut scissors" },
    IconEntry { glyph: "\u{F0193}", keywords: "content paste" },
    IconEntry { glyph: "\u{F0453}", keywords: "refresh reload sync" },
    IconEntry { glyph: "\u{F0450}", keywords: "delete trash" },
    IconEntry { glyph: "\u{F0493}", keywords: "dashboard home panel" },
    IconEntry { glyph: "\u{F02DC}", keywords: "home house residence" },
    IconEntry { glyph: "\u{F00F7}", keywords: "cancel close x" },
    IconEntry { glyph: "\u{F012C}", keywords: "check tick done ok" },
    IconEntry { glyph: "\u{F0133}", keywords: "close x remove" },
    IconEntry { glyph: "\u{F0156}", keywords: "minus remove subtract" },
    IconEntry { glyph: "\u{F0415}", keywords: "plus add new" },
    IconEntry { glyph: "\u{F0374}", keywords: "menu hamburger lines" },
    IconEntry { glyph: "\u{F01D9}", keywords: "dots horizontal menu kebab" },
    IconEntry { glyph: "\u{F01D8}", keywords: "dots vertical menu kebab" },
    IconEntry { glyph: "\u{F09DE}", keywords: "dots triangle" },
    IconEntry { glyph: "\u{F0141}", keywords: "chevron up arrow" },
    IconEntry { glyph: "\u{F0140}", keywords: "chevron down arrow" },
    IconEntry { glyph: "\u{F0142}", keywords: "chevron right arrow" },
    IconEntry { glyph: "\u{F0143}", keywords: "chevron left arrow" },
    IconEntry { glyph: "\u{F0054}", keywords: "arrow up" },
    IconEntry { glyph: "\u{F0045}", keywords: "arrow down" },
    IconEntry { glyph: "\u{F0054}", keywords: "arrow right" },
    IconEntry { glyph: "\u{F004D}", keywords: "arrow left" },
    IconEntry { glyph: "\u{F0453}", keywords: "sync reload refresh" },
    IconEntry { glyph: "\u{F04E0}", keywords: "update upgrade" },

    // Shapes / emoji / misc
    IconEntry { glyph: "\u{F02D7}", keywords: "heart love favorite" },
    IconEntry { glyph: "\u{F02D6}", keywords: "heart outline like" },
    IconEntry { glyph: "\u{F04CE}", keywords: "star favorite bookmark" },
    IconEntry { glyph: "\u{F04D2}", keywords: "star outline" },
    IconEntry { glyph: "\u{F04B3}", keywords: "flag mark flagged" },
    IconEntry { glyph: "\u{F023A}", keywords: "fire flame hot trending" },
    IconEntry { glyph: "\u{F00CE}", keywords: "bell ring notification" },
    IconEntry { glyph: "\u{F0335}", keywords: "lightbulb idea tip" },
    IconEntry { glyph: "\u{F002A}", keywords: "alert warning triangle" },
    IconEntry { glyph: "\u{F05D6}", keywords: "alert circle warning" },
    IconEntry { glyph: "\u{F02FC}", keywords: "information info circle" },
    IconEntry { glyph: "\u{F02D6}", keywords: "help question circle" },
    IconEntry { glyph: "\u{F0029}", keywords: "help question support" },
    IconEntry { glyph: "\u{F013F}", keywords: "coffee cup mug drink" },
    IconEntry { glyph: "\u{F02E6}", keywords: "food pizza" },
    IconEntry { glyph: "\u{F0F40}", keywords: "sword weapon game rpg" },
    IconEntry { glyph: "\u{F0498}", keywords: "shield check protect" },
    IconEntry { glyph: "\u{F02E6}", keywords: "target crosshair aim" },
    IconEntry { glyph: "\u{F0A0D}", keywords: "rocket launch ship" },
    IconEntry { glyph: "\u{F06E7}", keywords: "airplane travel flight" },
    IconEntry { glyph: "\u{F01B4}", keywords: "car vehicle" },
    IconEntry { glyph: "\u{F07BA}", keywords: "truck delivery vehicle" },
    IconEntry { glyph: "\u{F0158}", keywords: "map location route" },
    IconEntry { glyph: "\u{F034E}", keywords: "map marker location pin" },
    IconEntry { glyph: "\u{F0CD1}", keywords: "weather sun sunny day" },
    IconEntry { glyph: "\u{F0594}", keywords: "weather night moon dark" },
    IconEntry { glyph: "\u{F0596}", keywords: "weather cloudy cloud" },
    IconEntry { glyph: "\u{F0597}", keywords: "weather rainy rain" },
    IconEntry { glyph: "\u{F0598}", keywords: "weather snowy snow" },
    IconEntry { glyph: "\u{F059E}", keywords: "weather thunder lightning storm" },
    IconEntry { glyph: "\u{F0E4B}", keywords: "remote desktop monitor connection" },
    IconEntry { glyph: "\u{F02E6}", keywords: "target focus aim bullseye" },
    IconEntry { glyph: "\u{F0156}", keywords: "minimize window" },
    IconEntry { glyph: "\u{F05CC}", keywords: "account group team users people" },
    IconEntry { glyph: "\u{F0013}", keywords: "user person profile avatar" },
    IconEntry { glyph: "\u{F0E52}", keywords: "gamepad controller game" },
];

/// Search the catalog by case-insensitive keyword substring.
/// Empty query returns the full catalog.
pub fn search(query: &str) -> Vec<&'static IconEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return CATALOG.iter().collect();
    }
    CATALOG
        .iter()
        .filter(|e| e.keywords.contains(&q))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_returns_all_on_empty() {
        assert_eq!(search("").len(), CATALOG.len());
        assert_eq!(search("   ").len(), CATALOG.len());
    }

    #[test]
    fn test_search_matches_keyword() {
        let hits = search("chrome");
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|e| e.keywords.contains("chrome")));
    }

    #[test]
    fn test_search_case_insensitive() {
        let a = search("BROWSER").len();
        let b = search("browser").len();
        assert_eq!(a, b);
        assert!(a > 0);
    }

    #[test]
    fn test_search_no_match_returns_empty() {
        assert!(search("zzznosuchkeywordxyz").is_empty());
    }

    #[test]
    fn test_catalog_has_many_entries() {
        assert!(CATALOG.len() > 150, "catalog should contain many entries (got {})", CATALOG.len());
    }
}
