//! Menubar (tray) icon: a status badge plus quick actions.
//!
//! The icon is a template image (macOS recolors it to match the menubar) —
//! full-alpha bolt while connected, dimmed while not. The menu leads with a
//! read-only status line and a Connect/Disconnect toggle for the last-used
//! profile, so quick checks and toggles never need the main window.

use crate::runner::{self, StatusPayload};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

const ICON_ON: &[u8] = include_bytes!("../icons/tray-on.png");
const ICON_OFF: &[u8] = include_bytes!("../icons/tray-off.png");

/// Handles needed to update the tray after a status change.
struct TrayUi {
    tray: TrayIcon<Wry>,
    status_item: MenuItem<Wry>,
    toggle_item: MenuItem<Wry>,
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, "status", "Not connected", false, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, "toggle", "Connect", false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &PredefinedMenuItem::separator(app)?,
            &toggle_item,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Hysteria Client")
        .icon(Image::from_bytes(ICON_OFF)?)
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => crate::show_main_window(app),
            "toggle" => toggle_connection(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    app.manage(TrayUi { tray, status_item, toggle_item });
    Ok(())
}

/// Reflect a status change in the tray. Called from runner threads, so the
/// actual menu/icon mutation is dispatched to the main thread (required on
/// macOS).
pub fn update(app: &AppHandle, status: &StatusPayload) {
    let app2 = app.clone();
    let status = status.clone();
    let _ = app.run_on_main_thread(move || apply(&app2, &status));
}

fn apply(app: &AppHandle, status: &StatusPayload) {
    let Some(ui) = app.try_state::<TrayUi>() else {
        return;
    };
    let name = status.profile_name.clone().unwrap_or_default();
    let last_name = runner::last_connection(app).map(|(p, _)| p.name);

    let (icon, status_text, toggle_text, toggle_enabled) = match status.state.as_str() {
        "connected" => (ICON_ON, format!("Connected — {name}"), "Disconnect".to_string(), true),
        "connecting" => (
            ICON_OFF,
            format!("Connecting to {name}…"),
            "Cancel".to_string(),
            true,
        ),
        _ => {
            let label = match status.state.as_str() {
                "error" => "Connection error".to_string(),
                _ => "Not connected".to_string(),
            };
            match &last_name {
                Some(n) => (ICON_OFF, label, format!("Connect {n}"), true),
                None => (ICON_OFF, label, "Connect".to_string(), false),
            }
        }
    };

    if let Ok(img) = Image::from_bytes(icon) {
        let _ = ui.tray.set_icon(Some(img));
        let _ = ui.tray.set_icon_as_template(true);
    }
    let _ = ui.tray.set_tooltip(Some(format!("Hysteria — {status_text}")));
    let _ = ui.status_item.set_text(&status_text);
    let _ = ui.toggle_item.set_text(&toggle_text);
    let _ = ui.toggle_item.set_enabled(toggle_enabled);
}

/// Connect/disconnect from the tray. Runs off the main thread: connecting may
/// block on an osascript admin prompt, and disconnect waits on the child.
fn toggle_connection(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let state = runner::current_status(&app).state;
        match state.as_str() {
            "connected" | "connecting" => {
                let _ = runner::disconnect(&app);
            }
            _ => {
                if let Some((profile, mode)) = runner::last_connection(&app) {
                    let _ = runner::connect(&app, profile, &mode);
                }
            }
        }
    });
}
