use std::path::{Path, PathBuf};

use ms_manager_core::{ArtifactSource, FirmwareTarget};
use serde::Deserialize;

use crate::api_error::{ApiError, ApiResult};
use crate::services::artifact_paths::{dev_artifacts_path, ensure_file_exists};
use crate::services::artifact_resolver::{ArtifactHealth, DEV_ARTIFACTS_SCHEMA};

#[derive(Debug, Clone)]
pub struct WorkspaceArtifacts {
    pub config_path: PathBuf,
    pub strict: bool,
    pub oc_bridge_exe: PathBuf,
    pub loader_exe: PathBuf,
    pub firmware_standalone: PathBuf,
    pub firmware_bitwig: PathBuf,
    pub bitwig_extension: PathBuf,
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

pub fn load_workspace_artifacts() -> ApiResult<WorkspaceArtifacts> {
    load_workspace_artifacts_from_path(&dev_artifacts_path())
}

fn load_workspace_artifacts_from_path(path: &Path) -> ApiResult<WorkspaceArtifacts> {
    let bytes = std::fs::read(&path).map_err(|e| {
        ApiError::new(
            "artifact_config_missing",
            format!("workspace artifact config not found: {}", path.display()),
        )
        .with_details(
            serde_json::json!({ "path": path.display().to_string(), "io_error": e.to_string() }),
        )
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
        .ok_or_else(|| {
            ApiError::new(
                "artifact_config_invalid",
                "workspace config has no parent directory",
            )
        })?
        .to_path_buf();

    Ok(WorkspaceArtifacts {
        config_path: path.to_path_buf(),
        strict: file.strict,
        oc_bridge_exe: resolve_declared_path(
            &root,
            &file.artifacts.oc_bridge_exe,
            "oc_bridge_exe",
        )?,
        loader_exe: resolve_declared_path(&root, &file.artifacts.loader_exe, "loader_exe")?,
        firmware_standalone: resolve_declared_path(
            &root,
            &file.artifacts.firmware_standalone,
            "firmware_standalone",
        )?,
        firmware_bitwig: resolve_declared_path(
            &root,
            &file.artifacts.firmware_bitwig,
            "firmware_bitwig",
        )?,
        bitwig_extension: resolve_declared_path(
            &root,
            &file.artifacts.bitwig_extension,
            "bitwig_extension",
        )?,
    })
}

pub fn workspace_artifact_health() -> ArtifactHealth {
    match load_workspace_artifacts() {
        Ok(workspace) => {
            for (key, path) in [
                ("firmware_standalone", &workspace.firmware_standalone),
                ("firmware_bitwig", &workspace.firmware_bitwig),
            ] {
                if let Err(err) = ensure_file_exists(key, path) {
                    return ArtifactHealth {
                        source: ArtifactSource::Workspace,
                        ready: false,
                        config_path: Some(workspace.config_path.clone()),
                        message: Some(err.message),
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

pub fn workspace_artifact_health_for_target(target: FirmwareTarget) -> ArtifactHealth {
    match load_workspace_artifacts() {
        Ok(workspace) => {
            let (key, firmware) = match target {
                FirmwareTarget::Standalone => {
                    ("firmware_standalone", &workspace.firmware_standalone)
                }
                FirmwareTarget::Bitwig => ("firmware_bitwig", &workspace.firmware_bitwig),
            };

            if let Err(err) = ensure_file_exists(key, firmware) {
                return ArtifactHealth {
                    source: ArtifactSource::Workspace,
                    ready: false,
                    config_path: Some(workspace.config_path.clone()),
                    message: Some(err.message),
                };
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

    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(label: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ms-manager-{label}-{suffix}"))
    }

    #[test]
    fn load_workspace_artifacts_keeps_declared_paths_when_files_are_missing() {
        let root = unique_test_dir("workspace-artifacts");
        let config_path = root.join("dev-artifacts.local.json");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            &config_path,
            r#"{
  "schema": 1,
  "strict": true,
  "artifacts": {
    "oc_bridge_exe": "tools/oc-bridge.exe",
    "loader_exe": "tools/loader.exe",
    "firmware_standalone": "firmware/standalone/firmware.hex",
    "firmware_bitwig": "firmware/bitwig/firmware.hex",
    "bitwig_extension": "host/midi_studio.bwextension"
  }
}"#,
        )
        .unwrap();

        let workspace = load_workspace_artifacts_from_path(&config_path).unwrap();

        assert_eq!(
            workspace.firmware_standalone,
            root.join("firmware/standalone/firmware.hex")
        );
        assert_eq!(
            workspace.firmware_bitwig,
            root.join("firmware/bitwig/firmware.hex")
        );
    }
}
