use std::path::{Path, PathBuf};

use crate::api_error::{ApiError, ApiResult};

pub fn dev_artifacts_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri should have a repo root parent")
        .join("dev-artifacts.local.json")
}

pub fn ensure_file_exists(key: &str, path: &Path) -> ApiResult<()> {
    if path.exists() && path.is_file() {
        return Ok(());
    }

    Err(ApiError::new(
        "artifact_missing",
        format!("artifact '{key}' not found: {}", path.display()),
    )
    .with_details(serde_json::json!({
        "artifact": key,
        "path": path.display().to_string(),
    })))
}

pub fn ui_path_string(path: &Path) -> String {
    let raw = path.display().to_string();
    strip_windows_verbatim_prefix(&raw)
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = path.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{rest}");
        }
        if let Some(rest) = path.strip_prefix("\\\\?\\") {
            return rest.to_string();
        }
    }

    path.to_string()
}
