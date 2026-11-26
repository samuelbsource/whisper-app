//! Tray icon and menu. The tray app owns the process: quitting here unloads everything.

use crate::{Core, runtime::Activity};
use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

const TRAY_ID: &str = "main";
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");
const ICON_BUSY: &[u8] = include_bytes!("../icons/tray-busy.png");

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Win Whisper", true, None::<&str>)?;
    let copy = MenuItem::with_id(
        app,
        "copy_last",
        "Copy last transcription",
        true,
        None::<&str>,
    )?;
    let unload = MenuItem::with_id(app, "unload", "Unload all models", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Win Whisper", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &copy,
            &unload,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_IDLE)?)
        .tooltip("Win Whisper")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "copy_last" => copy_last(app),
            "unload" => {
                let core = crate::core(app);
                core.engine.unload_all();
                core.notify();
                refresh(&core);
            }
            "quit" => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Update the icon and tooltip to match what the app is doing.
pub fn refresh(core: &Core) {
    let Some(tray) = core.app.tray_by_id(TRAY_ID) else {
        return;
    };
    let (icon, tooltip) = match core.runtime.activity() {
        Activity::Recording => (ICON_RECORDING, "Win Whisper: recording".to_string()),
        Activity::Busy => (ICON_BUSY, "Win Whisper: transcribing".to_string()),
        Activity::Idle if core.engine.has_loaded() => {
            (ICON_IDLE, "Win Whisper: model loaded".to_string())
        }
        Activity::Idle => (ICON_IDLE, "Win Whisper".to_string()),
    };
    if let Ok(image) = Image::from_bytes(icon) {
        let _ = tray.set_icon(Some(image));
    }
    let _ = tray.set_tooltip(Some(tooltip));
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn copy_last(app: &AppHandle) {
    let core = crate::core(app);
    let Some(text) = core.last_text.lock().unwrap().clone() else {
        return;
    };
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}

pub fn quit_app(app: &AppHandle) {
    crate::core(app).shutdown();
    app.exit(0);
}
