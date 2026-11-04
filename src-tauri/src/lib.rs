mod config;
mod model;
mod runner;
mod store;
mod tray;
mod uri;

use model::{HysteriaConfig, Profile};
use runner::{RunnerState, StatusPayload};
use tauri::{AppHandle, Manager};

// --- config / URI commands ---

/// Parse a `hysteria2://` / `hy2://` URI into a profile.
#[tauri::command]
fn import_uri(uri: String) -> Result<Profile, String> {
    uri::parse_uri(&uri)
}

/// Build a canonical share URI from a profile.
#[tauri::command]
fn export_uri(profile: Profile) -> Result<String, String> {
    Ok(uri::build_uri(&profile))
}

/// Render a profile's config to Hysteria2 client YAML, exactly as `connect`
/// would write it for `mode` — the mode-derived fields (tun, proxy listener
/// fallback) only exist after `apply_mode`, so the preview must go through it
/// too or it silently omits everything the Client settings window edits.
#[tauri::command]
fn generate_yaml(mut config: HysteriaConfig, mode: String) -> Result<String, String> {
    runner::apply_mode(&mut config, &mode)?;
    config::generate_yaml(&config)
}

// --- profile persistence commands ---

#[tauri::command]
fn list_profiles(app: AppHandle) -> Result<Vec<Profile>, String> {
    store::load(&app)
}

#[tauri::command]
fn save_profile(app: AppHandle, profile: Profile) -> Result<Vec<Profile>, String> {
    store::upsert(&app, profile)
}

#[tauri::command]
fn delete_profile(app: AppHandle, id: String) -> Result<Vec<Profile>, String> {
    store::delete(&app, &id)
}

// --- connection commands ---

#[tauri::command]
fn connect(app: AppHandle, profile: Profile, mode: String) -> Result<(), String> {
    runner::connect(&app, profile, &mode)
}

#[tauri::command]
fn disconnect(app: AppHandle) -> Result<(), String> {
    runner::disconnect(&app)
}

#[tauri::command]
fn connection_status(app: AppHandle) -> StatusPayload {
    runner::current_status(&app)
}

/// Version + path of the hysteria binary the app runs.
#[tauri::command]
async fn hysteria_version(app: AppHandle) -> Result<runner::VersionInfo, String> {
    runner::hysteria_version(&app)
}

/// Externally visible IP. Async so the up-to-5s curl doesn't block the main
/// thread (Tauri runs async commands on a worker pool).
#[tauri::command]
async fn external_ip(app: AppHandle) -> Result<String, String> {
    runner::external_ip(&app)
}

// --- privileged helper (passwordless TUN) ---

#[tauri::command]
fn privilege_status(app: AppHandle) -> bool {
    runner::privilege_status(&app)
}

#[tauri::command]
fn install_privileges(app: AppHandle) -> Result<(), String> {
    runner::install_privileges(&app)
}

#[tauri::command]
fn uninstall_privileges() -> Result<(), String> {
    runner::uninstall_privileges()
}

/// Bring the main window back after it was hidden into the tray. On macOS this
/// also restores the Dock icon (see the CloseRequested handler below).
pub(crate) fn show_main_window(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(RunnerState::default())
        .setup(|app| {
            tray::init(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Secondary windows (config-*) close normally; only the main window
            // hides into the tray.
            if window.label() != "main" {
                return;
            }
            // Closing the window keeps the app alive in the tray; on macOS the
            // Dock icon is dropped too (Accessory policy) so only the menubar
            // icon remains. Quit via the tray menu or Cmd+Q.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                // Settings windows edit the main window's draft — pointless
                // (and confusing) to leave them up without it.
                for (label, w) in window.app_handle().webview_windows() {
                    if label != "main" {
                        let _ = w.close();
                    }
                }
                #[cfg(target_os = "macos")]
                let _ = window
                    .app_handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        })
        .invoke_handler(tauri::generate_handler![
            import_uri,
            export_uri,
            generate_yaml,
            list_profiles,
            save_profile,
            delete_profile,
            connect,
            disconnect,
            connection_status,
            hysteria_version,
            external_ip,
            privilege_status,
            install_privileges,
            uninstall_privileges
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Last chance to stop the hysteria child so it doesn't outlive the
            // app. Fires on every normal quit path (tray Quit, Cmd+Q).
            if let tauri::RunEvent::Exit = event {
                let _ = runner::disconnect(app);
            }
        });
}
