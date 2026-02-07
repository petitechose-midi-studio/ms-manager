mod api_error;
mod commands;
mod layout;
mod models;
mod services;
mod state;
mod storage;

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Ensure the bundle type marker is linked into the binary so the bundler can patch it.
            // (Otherwise, `__TAURI_BUNDLE_TYPE` may be stripped by the linker in some builds.)
            let _ = tauri::utils::platform::bundle_type();

            let state = state::AppState::load(&app.handle())?;
            app.manage(state);

            // End-user default: ms-manager starts at login (per-user).
            // Best-effort: ignore failures so the app remains bootable.
            if !cfg!(debug_assertions) {
                if !services::manager_autostart::is_installed() {
                    let _ = services::manager_autostart::install();
                }
            }

            // Tray: keep the app running in the background.
            // This is the foundation for a "start at login" UX.
            let show = MenuItemBuilder::new("Open ms-manager")
                .id("show")
                .build(app)?;
            let quit = MenuItemBuilder::new("Quit").id("quit").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

            TrayIconBuilder::new()
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            // Best-effort: ask the daemon to exit cleanly.
                            let _ = services::bridge_ctl::send_command(
                                services::bridge_ctl::DEFAULT_CONTROL_PORT,
                                "shutdown",
                                std::time::Duration::from_millis(700),
                            )
                            .await;

                            // Give it a moment to release ports/lock.
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;

                            // Strict: ensure no oc-bridge daemon is left running.
                            let layout = app.state::<state::AppState>().layout_get();
                            let exe = services::payload::oc_bridge_path(&layout);
                            let _ = services::bridge_process::kill_oc_bridge_daemons(&exe);
                            let _ = services::bridge_process::kill_all_oc_bridge_daemons();

                            app.exit(0);
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::DoubleClick { .. } = ev {
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // If launched in background mode (e.g., autostart), don't show the window.
            let background = std::env::args().any(|a| a == "--background");
            if background {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }

            // Best-effort: keep oc-bridge running in the background for the user session.
            // (No-op when the host bundle is not installed.)
            services::bridge::spawn_bridge_supervisor(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_update::app_update_check,
            commands::app_update::app_update_open_latest,
            commands::distribution::resolve_latest_manifest,
            commands::distribution::resolve_manifest_for_tag,
            commands::distribution::list_channel_tags,
            commands::distribution::plan_latest_install,
            commands::autostart::manager_autostart_get,
            commands::autostart::manager_autostart_set,
            commands::bridge::bridge_status_get,
            commands::bridge::bridge_log_open,
            commands::device::device_status_get,
            commands::flash::flash_firmware,
            commands::install::install_latest,
            commands::install::install_selected,
            commands::payload::payload_root_open,
            commands::payload::payload_root_relocate,
            commands::settings::settings_get,
            commands::settings::settings_set_channel,
            commands::settings::settings_set_profile,
            commands::settings::settings_set_pinned_tag,
            commands::status::status_get,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
