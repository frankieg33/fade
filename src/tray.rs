/// System tray icon and context menu.
/// Uses tray-icon + muda crates. Created on the main thread (same as Slint event loop).

/// Tray event types that main.rs cares about.
#[allow(dead_code)]
pub enum TrayAction {
    ShowSettings,
    TogglePause,
    Quit,
    None,
}

/// Opaque handle to keep the tray icon alive.
pub struct TrayHandle {
    #[cfg(target_os = "windows")]
    _icon: tray_icon::TrayIcon,
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use muda::{Menu, MenuItem, PredefinedMenuItem, CheckMenuItem, MenuEvent};
    use tray_icon::{TrayIconBuilder, TrayIconEvent, Icon};

    pub const MENU_SETTINGS: &str = "settings";
    pub const MENU_PAUSE: &str = "pause";
    pub const MENU_QUIT: &str = "quit";

    pub fn create_tray_icon(icon_rgba: Vec<u8>, width: u32, height: u32) -> Result<TrayHandle, String> {
        let icon = Icon::from_rgba(icon_rgba, width, height)
            .map_err(|e| format!("Failed to create icon: {}", e))?;

        let menu = Menu::new();

        let label = MenuItem::with_id(MENU_SETTINGS, "Settings...", true, None);
        let pause = CheckMenuItem::with_id(MENU_PAUSE, "Paused", true, false, None);
        let quit = MenuItem::with_id(MENU_QUIT, "Quit Fade", true, None);

        menu.append(&label).map_err(|e| format!("Menu error: {}", e))?;
        menu.append(&PredefinedMenuItem::separator()).map_err(|e| format!("Menu error: {}", e))?;
        menu.append(&pause).map_err(|e| format!("Menu error: {}", e))?;
        menu.append(&PredefinedMenuItem::separator()).map_err(|e| format!("Menu error: {}", e))?;
        menu.append(&quit).map_err(|e| format!("Menu error: {}", e))?;

        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_tooltip("Fade — Idle Window Manager")
            .build()
            .map_err(|e| format!("Failed to create tray icon: {}", e))?;

        Ok(TrayHandle { _icon: tray })
    }

    pub fn poll_tray_events() -> TrayAction {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            return match event.id().0.as_str() {
                MENU_SETTINGS => TrayAction::ShowSettings,
                MENU_PAUSE => TrayAction::TogglePause,
                MENU_QUIT => TrayAction::Quit,
                _ => TrayAction::None,
            };
        }

        if let Ok(event) = TrayIconEvent::receiver().try_recv() {
            return match event {
                TrayIconEvent::DoubleClick { .. } => TrayAction::ShowSettings,
                _ => TrayAction::None,
            };
        }

        TrayAction::None
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;

    pub fn create_tray_icon(_icon_rgba: Vec<u8>, _width: u32, _height: u32) -> Result<TrayHandle, String> {
        Err("Tray icon not supported on this platform".into())
    }

    pub fn poll_tray_events() -> TrayAction {
        TrayAction::None
    }
}

pub use platform::*;

/// Load the embedded Fade icon PNG as RGBA bytes for the tray icon.
#[cfg(target_os = "windows")]
pub fn load_icon() -> (Vec<u8>, u32, u32) {
    let png_bytes = include_bytes!("../ui/Fade Icon.png");
    match image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png) {
        Ok(img) => {
            let rgba_img = img.to_rgba8();
            let width = rgba_img.width();
            let height = rgba_img.height();
            (rgba_img.into_raw(), width, height)
        }
        Err(e) => {
            log::warn!("Failed to load embedded icon: {}, using fallback", e);
            generate_fallback_icon()
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn load_icon() -> (Vec<u8>, u32, u32) {
    generate_fallback_icon()
}

/// Simple solid color square as RGBA bytes — fallback if PNG fails to load.
fn generate_fallback_icon() -> (Vec<u8>, u32, u32) {
    let size: u32 = 32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for i in 0..(size * size) as usize {
        rgba[i * 4] = 137;     // R — #89b4fa
        rgba[i * 4 + 1] = 180; // G
        rgba[i * 4 + 2] = 250; // B
        rgba[i * 4 + 3] = 255; // A
    }
    (rgba, size, size)
}
