use serde::Serialize;
use tauri::State;

use ms_manager_core::{
    asset_url_for_tag, select_default_assets, Channel, Manifest, Platform,
};

use crate::api_error::{ApiError, ApiResult};
use crate::services::distribution;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct LatestManifestResponse {
    pub channel: Channel,
    pub available: bool,
    pub tag: Option<String>,
    pub manifest: Option<Manifest>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetPlan {
    pub id: String,
    pub filename: String,
    pub sha256: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub channel: Channel,
    pub tag: String,
    pub platform: Platform,
    pub assets: Vec<AssetPlan>,
}

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
    state: State<'_, AppState>,
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

    let platform = Platform::current()?;
    let assets = select_default_assets(&manifest, platform.os.as_str(), platform.arch.as_str())?;
    let plans = assets
        .into_iter()
        .map(|a| AssetPlan {
            id: a.id,
            filename: a.filename.clone(),
            sha256: a.sha256,
            size: a.size,
            url: a.url.unwrap_or_else(|| asset_url_for_tag(&tag, &a.filename)),
        })
        .collect::<Vec<_>>();

    Ok(InstallPlan {
        channel,
        tag,
        platform,
        assets: plans,
    })
}
