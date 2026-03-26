use tauri::Emitter;
use tauri::State;

use ms_manager_core::{compare_tags, Channel, InstallState, INSTALL_STATE_SCHEMA};

use crate::api_error::{ApiError, ApiResult};
use crate::commands::distribution::plan_install_internal;
use crate::models::InstallEvent;
use crate::services::{assets, bridge_ctl, install};
use crate::state::AppState;

const INSTALL_EVENT: &str = "ms-manager://install";

#[tauri::command]
pub async fn install_bridge_instance(
    instance_id: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> ApiResult<InstallState> {
    let binding = state
        .bridge_instances_get()
        .instances
        .into_iter()
        .find(|binding| binding.instance_id == instance_id)
        .ok_or_else(|| {
            ApiError::new(
                "bridge_instance_not_found",
                format!("unknown instance_id: {instance_id}"),
            )
        })?;

    if binding.artifact_source != ms_manager_core::ArtifactSource::Installed {
        return Err(ApiError::new(
            "bridge_instance_install_invalid",
            "instance does not use installed artifacts",
        ));
    }

    let channel = binding.installed_channel.ok_or_else(|| {
        ApiError::new(
            "bridge_instance_install_invalid",
            "installed channel is missing for this instance",
        )
    })?;

    let profile = binding.target.profile_id().to_string();
    let plan = plan_install_internal(
        channel,
        &profile,
        binding.installed_pinned_tag.as_deref(),
        &state,
    )
    .await?;
    let allow_downgrade = binding.installed_pinned_tag.is_some();
    let installed =
        install_from_plan(channel, plan.clone(), allow_downgrade, false, &app, &state).await?;
    let _ = state.bridge_instance_set_installed_release(
        &binding.instance_id,
        channel,
        Some(plan.tag),
    )?;
    Ok(installed)
}

async fn install_from_plan(
    channel: Channel,
    plan: crate::models::InstallPlan,
    allow_downgrade: bool,
    activate_current: bool,
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

    if activate_current {
        shutdown_active_bridges(state).await;
    }

    let installed = install::apply_install(&layout, &plan, &cached, activate_current).await?;

    let next = InstallState {
        schema: INSTALL_STATE_SCHEMA,
        channel,
        profile: installed.profile,
        tag: installed.tag,
    };
    let next = if activate_current {
        state.install_state_set(next)?
    } else {
        next
    };

    let _ = app.emit(
        INSTALL_EVENT,
        InstallEvent::Done {
            tag: next.tag.clone(),
            profile: next.profile.clone(),
        },
    );

    Ok(next)
}

async fn shutdown_active_bridges(state: &AppState) {
    let mut ports = state
        .bridge_instances_get()
        .instances
        .into_iter()
        .filter(|binding| binding.enabled)
        .map(|binding| binding.control_port)
        .collect::<Vec<_>>();

    ports.sort_unstable();
    ports.dedup();

    for port in ports {
        let _ = bridge_ctl::send_command(port, "shutdown", std::time::Duration::from_secs(2)).await;
    }

    if !state.bridge_instances_get().instances.is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    }
}
