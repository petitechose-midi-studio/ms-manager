use std::path::{Path, PathBuf};

use ms_manager_core::{ArtifactSource, InstallState, Settings};
use serde::Deserialize;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;

pub const DEV_ARTIFACTS_SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub struct ArtifactHealth {
    pub source: ArtifactSource,
    pub ready: bool,
    pub config_path: Option<PathBuf>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkspaceArtifacts {
    config_path: PathBuf,
    strict: bool,
    oc_bridge_exe: PathBuf,
    loader_exe: PathBuf,
    firmware_standalone: PathBuf,
    firmware_bitwig: PathBuf,
    #[allow(dead_code)]
    bitwig_extension: PathBuf,
}

#[derive(Debug, Deserialize)]
struct DevArtifactsFile {
    schema: u32,
    #[serde(default = "default_true")]
    strict: bool,
    artifacts: DevArtifactsMap,
}

#[derive(Debug, Deserialize)]
struct DevArtifactsMap {
    oc_bridge_exe: String,
    loader_exe: String,
    firmware_standalone: String,
    firmware_bitwig: String,
    bitwig_extension: String,
}

fn default_true() -> bool {
    true
}

pub fn dev_artifacts_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri should have a repo root parent")
        .join("dev-artifacts.local.json")
}

pub fn artifact_health(
    settings: &Settings,
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
) -> ArtifactHealth {
    match settings.artifact_source {
        ArtifactSource::Installed => installed_artifact_health(layout, installed),
        ArtifactSource::Workspace => workspace_artifact_health(settings),
    }
}

pub fn resolve_oc_bridge_exe(settings: &Settings, layout: &PayloadLayout) -> ApiResult<PathBuf> {
    match settings.artifact_source {
        ArtifactSource::Installed => {
            let path = installed_oc_bridge_exe(layout);
            ensure_file_exists("oc_bridge_exe", &path)?;
            Ok(path)
        }
        ArtifactSource::Workspace => Ok(load_workspace_artifacts()?.oc_bridge_exe),
    }
}

pub fn resolve_loader_exe(settings: &Settings, layout: &PayloadLayout) -> ApiResult<PathBuf> {
    match settings.artifact_source {
        ArtifactSource::Installed => {
            let path = installed_loader_exe(layout);
            ensure_file_exists("loader_exe", &path)?;
            Ok(path)
        }
        ArtifactSource::Workspace => Ok(load_workspace_artifacts()?.loader_exe),
    }
}

pub fn resolve_firmware_for_profile(
    settings: &Settings,
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    profile: &str,
) -> ApiResult<PathBuf> {
    let profile = profile.trim();
    if profile.is_empty() {
        return Err(ApiError::new("invalid_profile", "profile cannot be empty"));
    }

    match settings.artifact_source {
        ArtifactSource::Installed => {
            let installed = installed.ok_or_else(|| {
                ApiError::new("artifact_missing", "no installed host payload available")
            })?;
            installed_firmware_for_profile(layout, &installed.tag, profile)
        }
        ArtifactSource::Workspace => {
            let workspace = load_workspace_artifacts()?;
            match profile {
                "default" => Ok(workspace.firmware_standalone),
                "bitwig" => Ok(workspace.firmware_bitwig),
                other => Err(ApiError::new(
                    "artifact_missing",
                    format!("workspace firmware is not configured for profile '{other}'"),
                )),
            }
        }
    }
}

#[allow(dead_code)]
pub fn resolve_bitwig_extension(settings: &Settings) -> ApiResult<PathBuf> {
    match settings.artifact_source {
        ArtifactSource::Installed => Err(ApiError::new(
            "artifact_unsupported",
            "installed bitwig extension resolution is not implemented in this path",
        )),
        ArtifactSource::Workspace => Ok(load_workspace_artifacts()?.bitwig_extension),
    }
}

fn installed_artifact_health(layout: &PayloadLayout, installed: Option<&InstallState>) -> ArtifactHealth {
    let bridge = installed_oc_bridge_exe(layout);
    let loader = installed_loader_exe(layout);

    if !bridge.exists() || !loader.exists() {
        return ArtifactHealth {
            source: ArtifactSource::Installed,
            ready: false,
            config_path: None,
            message: Some("host bundle is not installed".to_string()),
        };
    }

    let Some(installed) = installed else {
        return ArtifactHealth {
            source: ArtifactSource::Installed,
            ready: false,
            config_path: None,
            message: Some("host bundle is present but no install state is active".to_string()),
        };
    };

    match installed_firmware_for_profile(layout, &installed.tag, &installed.profile) {
        Ok(_) => ArtifactHealth {
            source: ArtifactSource::Installed,
            ready: true,
            config_path: None,
            message: None,
        },
        Err(err) => ArtifactHealth {
            source: ArtifactSource::Installed,
            ready: false,
            config_path: None,
            message: Some(err.message),
        },
    }
}

fn workspace_artifact_health(settings: &Settings) -> ArtifactHealth {
    match load_workspace_artifacts() {
        Ok(workspace) => {
            let firmware = match settings.profile.as_str() {
                "default" => Some(&workspace.firmware_standalone),
                "bitwig" => Some(&workspace.firmware_bitwig),
                _ => None,
            };

            if let Some(path) = firmware {
                if !path.exists() {
                    return ArtifactHealth {
                        source: ArtifactSource::Workspace,
                        ready: false,
                        config_path: Some(workspace.config_path),
                        message: Some(format!(
                            "workspace firmware missing for profile '{}': {}",
                            settings.profile,
                            path.display()
                        )),
                    };
                }
            }

            ArtifactHealth {
                source: ArtifactSource::Workspace,
                ready: true,
                config_path: Some(workspace.config_path),
                message: if workspace.strict {
                    None
                } else {
                    Some("workspace artifact config loaded (non-strict)".to_string())
                },
            }
        }
        Err(err) => ArtifactHealth {
            source: ArtifactSource::Workspace,
            ready: false,
            config_path: Some(dev_artifacts_path()),
            message: Some(err.message),
        },
    }
}

fn installed_oc_bridge_exe(layout: &PayloadLayout) -> PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("oc-bridge.exe")
    } else {
        bin.join("oc-bridge")
    }
}

fn installed_loader_exe(layout: &PayloadLayout) -> PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("midi-studio-loader.exe")
    } else {
        bin.join("midi-studio-loader")
    }
}

fn installed_firmware_for_profile(layout: &PayloadLayout, tag: &str, profile: &str) -> ApiResult<PathBuf> {
    let dir = layout.version_dir(tag).join("firmware");
    if !dir.exists() {
        return Err(ApiError::new(
            "artifact_missing",
            "firmware directory not found for installed version",
        )
        .with_details(serde_json::json!({"dir": dir.display().to_string()})));
    }

    let mut candidates = Vec::<PathBuf>::new();
    let read = std::fs::read_dir(&dir)
        .map_err(|e| ApiError::new("io_read_failed", format!("read dir {}: {e}", dir.display())))?;
    for entry in read {
        let entry = entry.map_err(|e| ApiError::new("io_read_failed", e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).unwrap_or("") != "hex" {
            continue;
        }
        candidates.push(path);
    }

    let needle = profile.to_lowercase();
    let mut matches = candidates
        .into_iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase()
                .contains(&needle)
        })
        .collect::<Vec<_>>();

    if matches.len() == 1 {
        return Ok(matches.remove(0));
    }

    Err(ApiError::new(
        "artifact_missing",
        "firmware for selected profile is not installed (install that profile first)",
    )
    .with_details(serde_json::json!({
        "profile": profile,
        "dir": dir.display().to_string(),
    })))
}

fn load_workspace_artifacts() -> ApiResult<WorkspaceArtifacts> {
    let path = dev_artifacts_path();
    let bytes = std::fs::read(&path).map_err(|e| {
        ApiError::new(
            "artifact_config_missing",
            format!("workspace artifact config not found: {}", path.display()),
        )
        .with_details(serde_json::json!({ "path": path.display().to_string(), "io_error": e.to_string() }))
    })?;

    let file: DevArtifactsFile = serde_json::from_slice(&bytes).map_err(|e| {
        ApiError::new(
            "artifact_config_invalid",
            format!("invalid workspace artifact config: {e}"),
        )
        .with_details(serde_json::json!({ "path": path.display().to_string() }))
    })?;

    if file.schema != DEV_ARTIFACTS_SCHEMA {
        return Err(ApiError::new(
            "artifact_config_invalid",
            format!(
                "unsupported workspace artifact config schema: expected {}, got {}",
                DEV_ARTIFACTS_SCHEMA, file.schema
            ),
        )
        .with_details(serde_json::json!({ "path": path.display().to_string() })));
    }

    let root = path
        .parent()
        .ok_or_else(|| ApiError::new("artifact_config_invalid", "workspace config has no parent directory"))?;

    let oc_bridge_exe = resolve_declared_path(root, &file.artifacts.oc_bridge_exe, "oc_bridge_exe")?;
    let loader_exe = resolve_declared_path(root, &file.artifacts.loader_exe, "loader_exe")?;
    let firmware_standalone =
        resolve_declared_path(root, &file.artifacts.firmware_standalone, "firmware_standalone")?;
    let firmware_bitwig =
        resolve_declared_path(root, &file.artifacts.firmware_bitwig, "firmware_bitwig")?;
    let bitwig_extension =
        resolve_declared_path(root, &file.artifacts.bitwig_extension, "bitwig_extension")?;

    Ok(WorkspaceArtifacts {
        config_path: path,
        strict: file.strict,
        oc_bridge_exe,
        loader_exe,
        firmware_standalone,
        firmware_bitwig,
        bitwig_extension,
    })
}

fn resolve_declared_path(root: &Path, raw: &str, key: &str) -> ApiResult<PathBuf> {
    let value = raw.trim();
    if value.is_empty() {
        return Err(ApiError::new(
            "artifact_config_invalid",
            format!("workspace artifact entry '{key}' is empty"),
        ));
    }

    let candidate = PathBuf::from(value);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };

    ensure_file_exists(key, &path)?;
    Ok(path)
}

fn ensure_file_exists(key: &str, path: &Path) -> ApiResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_health_is_not_ready_without_payload() {
        let settings = Settings::default();
        let layout = PayloadLayout::resolve(Some("C:\\missing-payload-root")).unwrap();
        let status = artifact_health(&settings, &layout, None);
        assert_eq!(status.source, ArtifactSource::Installed);
        assert!(!status.ready);
    }
}
