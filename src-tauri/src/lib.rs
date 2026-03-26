mod api_error;
mod commands;
mod layout;
mod models;
mod services;
mod state;
mod storage;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            // Ensure the bundle type marker is linked into the binary so the bundler can patch it.
            // (Otherwise, `__TAURI_BUNDLE_TYPE` may be stripped by the linker in some builds.)
            let _ = tauri::utils::platform::bundle_type();

            let state = state::AppState::load(&app.handle())?;
            app.manage(state);

            services::startup::spawn_autostart_install();
            services::tray::install(app)?;
            services::startup::apply_background_mode(app);
            services::startup::spawn_background_services(app.handle().clone());
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
            commands::distribution::list_channel_tags,
            commands::bridge::bridge_status_get,
            commands::bridge::bridge_log_open,
            commands::bridge_instances::bridge_instances_get,
            commands::bridge_instances::bridge_instance_bind,
            commands::bridge_instances::bridge_instance_remove,
            commands::bridge_instances::bridge_instance_enable_set,
            commands::bridge_instances::bridge_instance_target_set,
            commands::bridge_instances::bridge_instance_artifact_source_set,
            commands::bridge_instances::bridge_instance_installed_release_set,
            commands::bridge_instances::bridge_instance_name_set,
            commands::device::device_status_get,
            commands::flash::flash_bridge_instance,
            commands::install::install_bridge_instance,
            commands::payload::path_open,
            commands::payload::payload_root_relocate,
            commands::status::status_get,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, _event| {});
}
