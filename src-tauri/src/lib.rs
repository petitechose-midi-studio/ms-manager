mod api_error;
mod commands;
mod services;
mod state;
mod storage;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = state::AppState::load(&app.handle())?;
            app.manage(state);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::distribution::resolve_latest_manifest,
            commands::distribution::plan_latest_install,
            commands::settings::settings_get,
            commands::settings::settings_set_channel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
