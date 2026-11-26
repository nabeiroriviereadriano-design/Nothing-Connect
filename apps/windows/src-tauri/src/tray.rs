//! Zone de notification : ouvrir, envoyer des fichiers, suspendre, quitter complètement.

use crate::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

static HINT_SHOWN: AtomicBool = AtomicBool::new(false);

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Une seule fois par session : explique que l'application reste active.
pub fn first_hide_hint(app: &AppHandle) {
    if !HINT_SHOWN.swap(true, Ordering::SeqCst) {
        let _ = app
            .notification()
            .builder()
            .title("NOTHING CONNECT reste actif")
            .body("Il continue de recevoir fichiers et notifications. Pour l'arrêter : clic droit sur l'icône, puis Quitter complètement.")
            .show();
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Ouvrir NOTHING CONNECT", true, None::<&str>)?;
    let send = MenuItem::with_id(app, "send", "Envoyer des fichiers…", true, None::<&str>)?;
    let suspended = app.state::<AppState>().settings.read().unwrap().suspended;
    let pause = CheckMenuItem::with_id(app, "suspend", "Suspendre les connexions", true, suspended, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter complètement", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &send, &pause, &sep, &quit])?;
    let pause_item = pause.clone();

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("icône de l'application"))
        .tooltip("NOTHING CONNECT")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, ev| match ev.id().as_ref() {
            "open" => show_main(app),
            "send" => {
                show_main(app);
                let _ = app.emit("nc-pick-files", ());
            }
            "suspend" => {
                let state = app.state::<AppState>();
                let node = state.node.lock().unwrap();
                let mut s = node.settings();
                s.suspended = !s.suspended;
                let on = s.suspended;
                if node.set_settings(s.clone()).is_ok() {
                    *state.settings.write().unwrap() = s;
                    let _ = pause_item.set_checked(on);
                    let _ = app.emit("nc-settings-changed", ());
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
