use tauri::State;

use crate::api_error::{ApiError, ApiResult};
use crate::commands::status::status_get_internal;
use crate::layout::PayloadLayout;
use crate::models::Status;
use crate::services::payload;
use crate::state::AppState;
#[tauri::command]
pub fn payload_root_open(state: State<'_, AppState>) -> ApiResult<()> {
    let layout = state.layout_get();
    let root = layout.root();

    // Best-effort: ensure the folder exists so the explorer doesn't error.
    let _ = std::fs::create_dir_all(root);

    let mut cmd = if cfg!(windows) {
        std::process::Command::new("explorer")
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else {
        std::process::Command::new("xdg-open")
    };
    cmd.arg(root);

    cmd.spawn()
        .map_err(|e| ApiError::new("open_failed", format!("open {}: {e}", root.display())))?;

    Ok(())
}

#[tauri::command]
pub async fn payload_root_relocate(
    new_root: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<Status> {
    let new_root = new_root.trim().to_string();
    if new_root.is_empty() {
        return Err(ApiError::new(
            "payload_root_invalid",
            "new_root cannot be empty",
        ));
    }

    let old_layout = state.layout_get();
    let new_layout = PayloadLayout::resolve(Some(&new_root))?;

    // Stop bridge to release file locks.
    let _ = payload::oc_bridge_ctl_shutdown(&old_layout).await;

    if let Some(r) = payload::oc_bridge_kill_process().await? {
        if !r.ok {
            return Err(
                ApiError::new("process_kill_failed", "failed to stop oc-bridge process")
                    .with_details(serde_json::json!({
                        "exit_code": r.exit_code,
                        "stdout": r.stdout,
                        "stderr": r.stderr,
                    })),
            );
        }
    }

    if old_layout.root().exists() {
        let tag = state.install_state_get().map(|s| s.tag);
        payload::relocate_payload_root(old_layout.clone(), new_layout.clone(), tag).await?;
    }

    // Persist and activate the new root.
    let s = state.settings_set_payload_root_override(Some(new_root))?;
    let effective_layout = PayloadLayout::resolve(s.payload_root_override.as_deref())?;
    state.layout_set(effective_layout.clone());
    state.payload_state_reload()?;

    // Ensure the root exists so "Open" works immediately.
    let _ = std::fs::create_dir_all(effective_layout.root());

    // Note: oc-bridge no longer installs a system service. The running bridge (if any)
    // will be restarted by the app (or on next login) using the new payload path.

    let _ = app; // reserved: future progress events
    status_get_internal(&state).await
}
