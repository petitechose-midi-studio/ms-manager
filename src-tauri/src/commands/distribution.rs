use tauri::State;

use ms_manager_core::{asset_url_for_tag, select_install_set_assets, Channel, Platform};

use crate::api_error::{ApiError, ApiResult};
use crate::models::{AssetPlan, InstallPlan, LatestManifestResponse};
use crate::services::distribution;
use crate::state::AppState;

#[tauri::command]
pub async fn resolve_latest_manifest(
    channel: Channel,
    state: State<'_, AppState>,
) -> ApiResult<LatestManifestResponse> {
    let out = distribution::resolve_latest_manifest(&state.http, channel).await?;
    Ok(LatestManifestResponse {
        channel,
        available: out.available,
        tag: out.tag,
        manifest: out.manifest,
        message: out.message,
    })
}

#[tauri::command]
pub async fn plan_latest_install(
    channel: Channel,
    profile: String,
    state: State<'_, AppState>,
) -> ApiResult<InstallPlan> {
    plan_latest_install_internal(channel, &profile, &state).await
}

pub(crate) async fn plan_latest_install_internal(
    channel: Channel,
    profile: &str,
    state: &AppState,
) -> ApiResult<InstallPlan> {
    let out = distribution::resolve_latest_manifest(&state.http, channel).await?;
    if !out.available {
        return Err(ApiError::new(
            "no_release_available",
            out.message.unwrap_or_else(|| "no releases".to_string()),
        ));
    }

    let manifest = out
        .manifest
        .ok_or_else(|| ApiError::new("internal_error", "missing manifest"))?;
    let tag = out
        .tag
        .ok_or_else(|| ApiError::new("internal_error", "missing tag"))?;

    if profile.is_empty() {
        return Err(ApiError::new("invalid_profile", "profile cannot be empty"));
    }

    let platform = Platform::current()?;
    let assets = select_install_set_assets(
        &manifest,
        profile,
        platform.os.as_str(),
        platform.arch.as_str(),
    )?;

    let plans = assets
        .into_iter()
        .map(|a| AssetPlan {
            id: a.id,
            kind: a.kind,
            filename: a.filename.clone(),
            sha256: a.sha256,
            size: a.size,
            url: a.url.unwrap_or_else(|| asset_url_for_tag(&tag, &a.filename)),
        })
        .collect::<Vec<_>>();

    Ok(InstallPlan {
        channel,
        tag,
        profile: profile.to_string(),
        platform,
        assets: plans,
    })
}
