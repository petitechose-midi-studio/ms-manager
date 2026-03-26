use tauri::{App, AppHandle, Manager, Runtime};

use crate::services;

pub fn spawn_autostart_install() {
    if cfg!(debug_assertions) {
        return;
    }

    std::thread::spawn(|| {
        let autostart_installed = services::manager_autostart::is_installed();
        if !autostart_installed {
            let _ = services::manager_autostart::install();
        }
    });
}

pub fn apply_background_mode<R: Runtime>(app: &App<R>) {
    if !std::env::args().any(|arg| arg == "--background") {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

pub fn spawn_background_services(app: AppHandle) {
    services::bridge::spawn_bridge_supervisor(app.clone());
    services::bridge_logs::spawn_bridge_log_supervisor(app);
}
