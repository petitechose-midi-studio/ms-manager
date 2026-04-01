use std::path::PathBuf;

use ms_manager_core::{ArtifactSource, BridgeInstanceBinding, Channel, InstallState};

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::services::artifact_resolver::ArtifactHealth;

pub fn installed_artifact_health(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
) -> ArtifactHealth {
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

pub fn installed_artifact_health_for_binding(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> ArtifactHealth {
    let bridge = installed_oc_bridge_exe_for_tag(layout, binding.installed_pinned_tag.as_deref());
    let loader = installed_loader_exe_for_tag(layout, binding.installed_pinned_tag.as_deref());

    if !bridge.exists() || !loader.exists() {
        return ArtifactHealth {
            source: ArtifactSource::Installed,
            ready: false,
            config_path: None,
            message: Some("installed version is not available for this instance".to_string()),
        };
    }

    let tag = match resolve_installed_tag(binding.installed_pinned_tag.as_deref(), installed) {
        Ok(tag) => tag,
        Err(err) => {
            return ArtifactHealth {
                source: ArtifactSource::Installed,
                ready: false,
                config_path: None,
                message: Some(err.message),
            };
        }
    };

    match installed_firmware_for_profile(layout, tag, binding.target.profile_id()) {
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

pub fn installed_oc_bridge_exe(layout: &PayloadLayout) -> PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("oc-bridge.exe")
    } else {
        bin.join("oc-bridge")
    }
}

pub fn installed_oc_bridge_exe_for_tag(layout: &PayloadLayout, tag: Option<&str>) -> PathBuf {
    match tag
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(tag) => {
            let bin = layout.version_dir(tag).join("bin");
            if cfg!(windows) {
                bin.join("oc-bridge.exe")
            } else {
                bin.join("oc-bridge")
            }
        }
        None => installed_oc_bridge_exe(layout),
    }
}

pub fn installed_loader_exe(layout: &PayloadLayout) -> PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("midi-studio-loader.exe")
    } else {
        bin.join("midi-studio-loader")
    }
}

pub fn installed_loader_exe_for_tag(layout: &PayloadLayout, tag: Option<&str>) -> PathBuf {
    match tag
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(tag) => {
            let bin = layout.version_dir(tag).join("bin");
            if cfg!(windows) {
                bin.join("midi-studio-loader.exe")
            } else {
                bin.join("midi-studio-loader")
            }
        }
        None => installed_loader_exe(layout),
    }
}

pub fn installed_version_dir_for_binding(
    layout: &PayloadLayout,
    installed: Option<&InstallState>,
    binding: &BridgeInstanceBinding,
) -> PathBuf {
    match binding
        .installed_pinned_tag
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(tag) => layout.version_dir(tag),
        None => installed
            .filter(|state| {
                state.channel == binding.installed_channel.unwrap_or(Channel::Stable)
                    && state.profile == binding.target.profile_id()
            })
            .map(|state| layout.version_dir(&state.tag))
            .unwrap_or_else(|| layout.versions_dir()),
    }
}

pub fn resolve_installed_tag<'a>(
    binding_tag: Option<&'a str>,
    installed: Option<&'a InstallState>,
) -> ApiResult<&'a str> {
    if let Some(tag) = binding_tag
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return Ok(tag);
    }

    installed
        .map(|state| state.tag.as_str())
        .ok_or_else(|| ApiError::new("artifact_missing", "no installed host payload available"))
}

pub fn installed_firmware_for_profile(
    layout: &PayloadLayout,
    tag: &str,
    profile: &str,
) -> ApiResult<PathBuf> {
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
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
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
