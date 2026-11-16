//! Sistem tepsisi: ayarlar, adayı gizle/göster, sesler, Windows ile başlat ve çıkış.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::autostart::{self, Autostart};
use crate::{island, settings, sound, sources};

/// Ayarlar penceresinden yapılan değişikliğin menüdeki işarete yansıması için.
struct AutostartItem(CheckMenuItem<Wry>);
struct SoundItem(CheckMenuItem<Wry>);
struct SourcesItem(CheckMenuItem<Wry>);

pub fn init(app: &AppHandle, sound_on: bool) -> tauri::Result<()> {
    let start = autostart::status();
    let open = MenuItem::with_id(app, "settings", "Ayarlar…", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Adayı gizle", true, None::<&str>)?;
    let login = CheckMenuItem::with_id(app, "autostart", "Windows ile başlat", start.available, start.enabled, None::<&str>)?;
    let sound = CheckMenuItem::with_id(app, "sound", "Sesler", true, sound_on, None::<&str>)?;
    // Kaynaklar (Mail, GitHub) açıkken: işaretliyken hiçbir kaynak ağa çıkmaz (bu oturum için).
    let sources_on = settings::current(app).features.sources;
    let pause = CheckMenuItem::with_id(app, "sources", "Kaynakları duraklat", sources_on, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Çıkış", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &toggle, &sound, &pause, &login, &separator, &quit])?;
    app.manage(AutostartItem(login));
    app.manage(SoundItem(sound));
    app.manage(SourcesItem(pause));

    let mut builder = TrayIconBuilder::with_id("zodyvel").tooltip("Zodyvel").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "settings" => settings::open_window(app),
            "toggle" => {
                let hidden = island::toggle_user_hidden(app);
                let _ = toggle.set_text(if hidden { "Adayı göster" } else { "Adayı gizle" });
            }
            "autostart" => {
                // Menü işareti tıklamayla zaten değişti; gerçek durum kayıttan okunup geri yazılır.
                let wanted = !autostart::status().enabled;
                let _ = autostart::apply(app, wanted);
            }
            // İşaret, ayar kaydedilince `sync_sound` ile gerçek duruma getirilir.
            "sound" => sound::toggle(app),
            "sources" => {
                let paused = sources::toggle_paused(app);
                sync_sources(app, settings::current(app).features.sources, paused);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                settings::open_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn sync_autostart(app: &AppHandle, state: &Autostart) {
    if let Some(item) = app.try_state::<AutostartItem>() {
        let _ = item.0.set_checked(state.enabled);
    }
}

pub fn sync_sound(app: &AppHandle, on: bool) {
    if let Some(item) = app.try_state::<SoundItem>() {
        let _ = item.0.set_checked(on);
    }
}

/// Kaynaklar kapalıyken "Kaynakları duraklat" soluk durur.
pub fn sync_sources(app: &AppHandle, enabled: bool, paused: bool) {
    if let Some(item) = app.try_state::<SourcesItem>() {
        let _ = item.0.set_enabled(enabled);
        let _ = item.0.set_checked(paused);
    }
}
