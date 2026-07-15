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
    installed_bin(layout.current_dir().join("bin"), "oc-bridge")
}

pub fn installed_oc_bridge_exe_for_tag(layout: &PayloadLayout, tag: Option<&str>) -> PathBuf {
    match tag
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(tag) => installed_bin(layout.version_dir(tag).join("bin"), "oc-bridge"),
        None => installed_oc_bridge_exe(layout),
    }
}

pub fn installed_loader_exe(layout: &PayloadLayout) -> PathBuf {
    installed_bin(layout.current_dir().join("bin"), "midi-studio-loader")
}

pub fn installed_loader_exe_for_tag(layout: &PayloadLayout, tag: Option<&str>) -> PathBuf {
    match tag
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        Some(tag) => installed_bin(layout.version_dir(tag).join("bin"), "midi-studio-loader"),
        None => installed_loader_exe(layout),
    }
}

pub fn installed_core_file_tool_exe(layout: &PayloadLayout) -> PathBuf {
    installed_bin(layout.current_dir().join("bin"), "ms-core-file-tool")
}

fn installed_bin(bin: PathBuf, stem: &str) -> PathBuf {
    if cfg!(windows) {
        bin.join(format!("{stem}.exe"))
    } else {
        bin.join(stem)
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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use ms_manager_core::{BridgeApp, BridgeMode, FirmwareTarget, INSTALL_STATE_SCHEMA};

    use super::*;

    struct TestPayload {
        root: PathBuf,
        layout: PayloadLayout,
    }

    impl TestPayload {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "ms-manager-installed-artifacts-{}-{nonce}",
                std::process::id()
            ));
            let layout = PayloadLayout::resolve(Some(root.to_str().unwrap())).unwrap();
            Self { root, layout }
        }

        fn install_runtime(&self, tag: &str, profile: &str) {
            let bridge = installed_oc_bridge_exe_for_tag(&self.layout, Some(tag));
            let loader = installed_loader_exe_for_tag(&self.layout, Some(tag));
            std::fs::create_dir_all(bridge.parent().unwrap()).unwrap();
            std::fs::write(bridge, b"bridge").unwrap();
            std::fs::write(loader, b"loader").unwrap();

            let firmware = self
                .layout
                .version_dir(tag)
                .join("firmware")
                .join(format!("midi-studio-{profile}-firmware.hex"));
            std::fs::create_dir_all(firmware.parent().unwrap()).unwrap();
            std::fs::write(firmware, b"firmware").unwrap();
        }

        fn install_current_runtime(&self) {
            let bridge = installed_oc_bridge_exe(&self.layout);
            let loader = installed_loader_exe(&self.layout);
            std::fs::create_dir_all(bridge.parent().unwrap()).unwrap();
            std::fs::write(bridge, b"bridge").unwrap();
            std::fs::write(loader, b"loader").unwrap();
        }
    }

    impl Drop for TestPayload {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn binding_health_does_not_require_cold_core_file_tool() {
        let payload = TestPayload::new();
        let tag = "v0.1.0-beta.2";
        payload.install_runtime(tag, "default");

        let binding = BridgeInstanceBinding {
            instance_id: "teck".to_string(),
            display_name: Some("Teck".to_string()),
            app: BridgeApp::Bitwig,
            mode: BridgeMode::Hardware,
            controller_serial: "17081760".to_string(),
            controller_vid: 0x16c0,
            controller_pid: 0x0489,
            target: FirmwareTarget::Standalone,
            artifact_source: ArtifactSource::Installed,
            installed_channel: Some(Channel::Beta),
            installed_pinned_tag: Some(tag.to_string()),
            host_udp_port: 9000,
            control_port: 7999,
            log_broadcast_port: 9999,
            enabled: true,
        };

        assert!(!installed_bin(
            payload.layout.version_dir(tag).join("bin"),
            "ms-core-file-tool"
        )
        .exists());
        let health = installed_artifact_health_for_binding(&payload.layout, None, &binding);
        assert!(health.ready, "{:?}", health.message);
    }

    #[test]
    fn management_health_does_not_require_cold_core_file_tool() {
        let payload = TestPayload::new();
        let tag = "v0.1.0-beta.2";
        payload.install_current_runtime();
        payload.install_runtime(tag, "default");
        let installed = InstallState {
            schema: INSTALL_STATE_SCHEMA,
            channel: Channel::Beta,
            profile: "default".to_string(),
            tag: tag.to_string(),
        };

        assert!(!installed_core_file_tool_exe(&payload.layout).exists());
        let health = installed_artifact_health(&payload.layout, Some(&installed));
        assert!(health.ready, "{:?}", health.message);
    }
}
