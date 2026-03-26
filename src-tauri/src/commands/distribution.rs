use tauri::State;

use ms_manager_core::{asset_url_for_tag, select_install_set_assets, Channel, Platform};

use crate::api_error::{ApiError, ApiResult};
use crate::models::{AssetPlan, InstallPlan};
use crate::services::distribution;
use crate::state::AppState;

#[tauri::command]
pub async fn list_channel_tags(
    channel: Channel,
    state: State<'_, AppState>,
) -> ApiResult<Vec<String>> {
    distribution::list_tags_for_channel(&state.http, channel).await
}

pub(crate) async fn plan_install_internal(
    channel: Channel,
    profile: &str,
    tag: Option<&str>,
    state: &AppState,
) -> ApiResult<InstallPlan> {
    let out = match tag {
        Some(t) => distribution::resolve_manifest_for_tag(&state.http, channel, t).await?,
        None => distribution::resolve_latest_manifest(&state.http, channel).await?,
    };
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
            url: a
                .url
                .unwrap_or_else(|| asset_url_for_tag(&tag, &a.filename)),
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
