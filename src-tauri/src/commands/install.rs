use tauri::State;
use tauri::Emitter;

use ms_manager_core::{compare_tags, Channel, InstallState, INSTALL_STATE_SCHEMA};

use crate::api_error::{ApiError, ApiResult};
use crate::commands::distribution::{plan_install_internal, plan_latest_install_internal};
use crate::models::InstallEvent;
use crate::services::{assets, install};
use crate::state::AppState;

const INSTALL_EVENT: &str = "ms-manager://install";

#[tauri::command]
pub async fn install_latest(
    channel: Channel,
    profile: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<InstallState> {
    let plan = plan_latest_install_internal(channel, &profile, &state).await?;
    install_from_plan(channel, plan, false, &app, &state).await
}

#[tauri::command]
pub async fn install_selected(app: tauri::AppHandle, state: State<'_, AppState>) -> ApiResult<InstallState> {
    let s = state.settings_get();
    let plan = plan_install_internal(s.channel, &s.profile, s.pinned_tag.as_deref(), &state).await?;
    // When a tag is explicitly pinned, allow downgrades.
    let allow_downgrade = s.pinned_tag.is_some();
    install_from_plan(s.channel, plan, allow_downgrade, &app, &state).await
}

async fn install_from_plan(
    channel: Channel,
    plan: crate::models::InstallPlan,
    allow_downgrade: bool,
    app: &tauri::AppHandle,
    state: &AppState,
) -> ApiResult<InstallState> {

    let layout = state.layout_get();

    // Anti-rollback: default update path must never auto-downgrade.
    // Explicit pinning is treated as user intent, and may downgrade.
    if !allow_downgrade {
        if let Some(installed) = state.install_state_get() {
        if installed.channel == channel {
            let ord = compare_tags(channel, &plan.tag, &installed.tag).ok_or_else(|| {
                ApiError::new(
                    "tag_invalid",
                    format!(
                        "cannot compare tags for channel {}: {} vs {}",
                        channel.as_str(),
                        plan.tag,
                        installed.tag
                    ),
                )
            })?;

            if ord.is_lt() {
                return Err(ApiError::new(
                    "downgrade_refused",
                    format!(
                        "refusing downgrade: installed {} -> target {}",
                        installed.tag, plan.tag
                    ),
                ));
            }
        }
    }
    }

    let _ = app.emit(
        INSTALL_EVENT,
        InstallEvent::Begin {
            channel,
            tag: plan.tag.clone(),
            profile: plan.profile.clone(),
            assets_total: plan.assets.len(),
        },
    );

    let mut cached = Vec::with_capacity(plan.assets.len());
    for (i, a) in plan.assets.iter().enumerate() {
        let _ = app.emit(
            INSTALL_EVENT,
            InstallEvent::Downloading {
                index: i + 1,
                total: plan.assets.len(),
                asset_id: a.id.clone(),
                filename: a.filename.clone(),
            },
        );
        let p = assets::ensure_asset_cached(&state.http, &layout, a).await?;
        cached.push(assets::CachedAsset {
            plan: a.clone(),
            path: p,
        });
    }

    let _ = app.emit(
        INSTALL_EVENT,
        InstallEvent::Applying {
            step: "extract_and_stage".to_string(),
        },
    );

    let installed = install::apply_install(&layout, &plan, &cached).await?;

    let next = InstallState {
        schema: INSTALL_STATE_SCHEMA,
        channel,
        profile: installed.profile,
        tag: installed.tag,
    };
    let next = state.install_state_set(next)?;

    let _ = app.emit(
        INSTALL_EVENT,
        InstallEvent::Done {
            tag: next.tag.clone(),
            profile: next.profile.clone(),
        },
    );

    Ok(next)
}
