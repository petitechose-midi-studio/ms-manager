use std::path::PathBuf;

use ms_manager_core::{ArtifactSource, BridgeInstanceBinding, FirmwareTarget, InstallState};

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::services::artifact_paths::{
    dev_artifacts_path as local_dev_artifacts_path, ensure_file_exists,
    ui_path_string as format_ui_path_string,
};
use crate::services::installed_artifacts::{
    installed_artifact_health, installed_artifact_health_for_binding, installed_firmware_for_profile,
    installed_loader_exe, installed_loader_exe_for_tag, installed_oc_bridge_exe,
    installed_oc_bridge_exe_for_tag, installed_version_dir_for_binding, resolve_installed_tag,
};
use crate::services::workspace_artifacts::{
    load_workspace_artifacts, workspace_artifact_health, workspace_artifact_health_for_target,
};

pub const DEV_ARTIFACTS_SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub struct ArtifactHealth {
    pub source: ArtifactSource,
    pub ready: bool,
    pub config_path: Option<PathBuf>,
    pub message: Option<String>,
}

pub fn management_artifact_health(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
) -> ArtifactHealth {
    if local_dev_artifacts_path().exists() {
        return workspace_artifact_health();
    }

    installed_artifact_health(layout, installed)
}

pub fn artifact_health_for_binding(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> ArtifactHealth {
    match binding.artifact_source {
        ArtifactSource::Installed => installed_artifact_health_for_binding(layout, installed, binding),
        ArtifactSource::Workspace => workspace_artifact_health_for_target(binding.target),
    }
}

pub fn resolve_management_oc_bridge_exe(layout: &PayloadLayout) -> ApiResult<PathBuf> {
    if let Ok(workspace) = load_workspace_artifacts() {
        return Ok(workspace.oc_bridge_exe);
    }

    let path = installed_oc_bridge_exe(layout);
    ensure_file_exists("oc_bridge_exe", &path)?;
    Ok(path)
}

pub fn resolve_oc_bridge_exe_for_binding(
    layout: &PayloadLayout,
    binding: &BridgeInstanceBinding,
) -> ApiResult<PathBuf> {
    match binding.artifact_source {
        ArtifactSource::Installed => {
            let path = installed_oc_bridge_exe_for_tag(layout, binding.installed_pinned_tag.as_deref());
            ensure_file_exists("oc_bridge_exe", &path)?;
            Ok(path)
        }
        ArtifactSource::Workspace => Ok(load_workspace_artifacts()?.oc_bridge_exe),
    }
}

pub fn resolve_management_loader_exe(layout: &PayloadLayout) -> ApiResult<PathBuf> {
    if let Ok(workspace) = load_workspace_artifacts() {
        return Ok(workspace.loader_exe);
    }

    let path = installed_loader_exe(layout);
    ensure_file_exists("loader_exe", &path)?;
    Ok(path)
}

pub fn resolve_loader_exe_for_binding(
    layout: &PayloadLayout,
    binding: &BridgeInstanceBinding,
) -> ApiResult<PathBuf> {
    match binding.artifact_source {
        ArtifactSource::Installed => {
            let path = installed_loader_exe_for_tag(layout, binding.installed_pinned_tag.as_deref());
            ensure_file_exists("loader_exe", &path)?;
            Ok(path)
        }
        ArtifactSource::Workspace => Ok(load_workspace_artifacts()?.loader_exe),
    }
}

pub fn resolve_firmware_for_binding(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> ApiResult<PathBuf> {
    match binding.artifact_source {
        ArtifactSource::Installed => {
            let tag = resolve_installed_tag(binding.installed_pinned_tag.as_deref(), installed)?;
            installed_firmware_for_profile(layout, tag, binding.target.profile_id())
        }
        ArtifactSource::Workspace => resolve_firmware_for_target(
            layout,
            installed,
            binding.artifact_source,
            binding.target,
        ),
    }
}

pub fn artifact_location_for_binding(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> PathBuf {
    match binding.artifact_source {
        ArtifactSource::Installed => installed_version_dir_for_binding(layout, installed, binding),
        ArtifactSource::Workspace => resolve_firmware_for_binding(layout, installed, binding)
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
            .unwrap_or_else(|| {
                local_dev_artifacts_path()
                    .parent()
                    .map(|parent| {
                        let path = parent.to_path_buf();
                        std::fs::canonicalize(&path).unwrap_or(path)
                    })
                    .unwrap_or_else(local_dev_artifacts_path)
            }),
    }
}

pub fn ui_path_string(path: &std::path::Path) -> String {
    format_ui_path_string(path)
}

pub fn resolve_firmware_for_target(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    artifact_source: ArtifactSource,
    target: FirmwareTarget,
) -> ApiResult<PathBuf> {
    match artifact_source {
        ArtifactSource::Installed => {
            let installed = installed.ok_or_else(|| {
                ApiError::new("artifact_missing", "no installed host payload available")
            })?;
            installed_firmware_for_profile(layout, &installed.tag, target.profile_id())
        }
        ArtifactSource::Workspace => {
            let workspace = load_workspace_artifacts()?;
            match target {
                FirmwareTarget::Standalone => Ok(workspace.firmware_standalone),
                FirmwareTarget::Bitwig => Ok(workspace.firmware_bitwig),
            }
        }
    }
}

#[allow(dead_code)]
pub fn resolve_bitwig_extension() -> ApiResult<PathBuf> {
    Ok(load_workspace_artifacts()?.bitwig_extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_health_is_not_ready_without_payload() {
        let layout = PayloadLayout::resolve(Some("C:\\missing-payload-root")).unwrap();
        let status = management_artifact_health(&layout, None);
        assert_eq!(status.source, ArtifactSource::Installed);
        assert!(!status.ready);
    }
}
