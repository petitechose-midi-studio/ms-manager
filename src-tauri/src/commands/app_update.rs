use std::sync::Mutex;

use tauri::{AppHandle, State};

use crate::api_error::{ApiError, ApiResult};
use crate::models::{AppUpdateInfo, AppUpdateStatus};
use crate::state::AppState;

#[cfg(desktop)]
use tauri_plugin_updater::{Update, UpdaterExt};

#[cfg(desktop)]
pub struct PendingAppUpdate(pub Mutex<Option<Update>>);

#[cfg(not(desktop))]
pub struct PendingAppUpdate(pub ());

#[tauri::command]
pub async fn app_update_check(app: AppHandle, pending: State<'_, PendingAppUpdate>) -> ApiResult<AppUpdateStatus> {
    let current_version = app.package_info().version.to_string();

    #[cfg(not(desktop))]
    {
        let _ = pending;
        let _ = current_version;
        return Ok(AppUpdateStatus {
            current_version: "(unsupported)".to_string(),
            available: false,
            update: None,
            error: Some("updater is not supported on this platform".to_string()),
        });
    }

    #[cfg(desktop)]
    {
        let updater = match app.updater() {
            Ok(v) => v,
            Err(e) => {
                *pending.0.lock().unwrap() = None;
                return Ok(AppUpdateStatus {
                    current_version,
                    available: false,
                    update: None,
                    error: Some(e.to_string()),
                });
            }
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let info = AppUpdateInfo {
                    version: update.version.clone(),
                    pub_date: update.date.map(|d| d.to_string()),
                    notes: update.body.clone(),
                };
                *pending.0.lock().unwrap() = Some(update);
                Ok(AppUpdateStatus {
                    current_version,
                    available: true,
                    update: Some(info),
                    error: None,
                })
            }
            Ok(None) => {
                *pending.0.lock().unwrap() = None;
                Ok(AppUpdateStatus {
                    current_version,
                    available: false,
                    update: None,
                    error: None,
                })
            }
            Err(e) => {
                *pending.0.lock().unwrap() = None;
                Ok(AppUpdateStatus {
                    current_version,
                    available: false,
                    update: None,
                    error: Some(e.to_string()),
                })
            }
        }
    }
}

#[tauri::command]
pub async fn app_update_install(
    app: AppHandle,
    state: State<'_, AppState>,
    pending: State<'_, PendingAppUpdate>,
) -> ApiResult<()> {
    #[cfg(not(desktop))]
    {
        let _ = app;
        let _ = state;
        let _ = pending;
        return Err(ApiError::new(
            "app_update_unsupported",
            "updater is not supported on this platform",
        ));
    }

    #[cfg(desktop)]
    {
        let Some(update) = pending.0.lock().unwrap().take() else {
            return Err(ApiError::new("app_update_missing", "no pending app update"));
        };

        // Best-effort: shut down oc-bridge before updating the app.
        // (Not strictly required for the app update itself, but it reduces the risk of leaving
        // background processes in a weird state.)
        let _ = crate::services::bridge_ctl::send_command(
            crate::services::bridge_ctl::DEFAULT_CONTROL_PORT,
            "shutdown",
            std::time::Duration::from_millis(700),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;

        let layout = state.layout_get();
        let exe = crate::services::payload::oc_bridge_path(&layout);
        let _ = crate::services::bridge_process::kill_oc_bridge_daemons(&exe);
        let _ = crate::services::bridge_process::kill_all_oc_bridge_daemons();

        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| ApiError::new("app_update_failed", e.to_string()))?;

        // On Windows, the app is expected to exit during install.
        // On Unix, we can restart to complete the update.
        app.restart();

        #[allow(unreachable_code)]
        Ok(())
    }
}
