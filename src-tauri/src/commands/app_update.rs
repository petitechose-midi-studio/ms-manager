use std::time::Duration;

use serde::Deserialize;
use tauri::AppHandle;

use crate::api_error::{ApiError, ApiResult};
use crate::models::{AppUpdateInfo, AppUpdateStatus};

const RELEASES_API_URL: &str = "https://api.github.com/repos/petitechose-midi-studio/ms-manager/releases/latest";
const RELEASES_PAGE_URL: &str =
    "https://github.com/petitechose-midi-studio/ms-manager/releases/latest";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
}

fn normalize_tag(tag: &str) -> String {
    tag.strip_prefix('v').unwrap_or(tag).to_string()
}

async fn fetch_latest_release() -> Result<GitHubRelease, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent("ms-manager")
        .timeout(Duration::from_secs(10))
        .build()?
        .get(RELEASES_API_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<GitHubRelease>()
        .await
}

#[tauri::command]
pub async fn app_update_check(app: AppHandle) -> ApiResult<AppUpdateStatus> {
    let current_version = app.package_info().version.to_string();

    match fetch_latest_release().await {
        Ok(release) => {
            let latest_version = normalize_tag(&release.tag_name);
            Ok(AppUpdateStatus {
                current_version: current_version.clone(),
                available: latest_version != current_version,
                update: Some(AppUpdateInfo {
                    version: latest_version,
                    pub_date: release.published_at,
                    notes: release.body,
                    url: release.html_url,
                }),
                error: None,
            })
        }
        Err(e) => Ok(AppUpdateStatus {
            current_version,
            available: false,
            update: None,
            error: Some(format!("failed to check latest app release: {e}")),
        }),
    }
}

#[tauri::command]
pub fn app_update_open_latest() -> ApiResult<()> {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("explorer");
        c.arg(RELEASES_PAGE_URL);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(RELEASES_PAGE_URL);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(RELEASES_PAGE_URL);
        c
    };

    cmd.spawn().map_err(|e| {
        ApiError::new(
            "open_failed",
            format!("open latest release page {RELEASES_PAGE_URL}: {e}"),
        )
    })?;

    Ok(())
}
