use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ms_manager_core::{
    ArtifactSource, BridgeInstanceBinding, BridgeInstancesState, Channel, ControllerState,
    FirmwareTarget, InstallState, LastFlashed, Settings, BRIDGE_INSTANCES_SCHEMA,
    CONTROLLER_STATE_SCHEMA, INSTALL_STATE_SCHEMA, SETTINGS_SCHEMA,
};
use reqwest::Client;
use tauri::path::BaseDirectory;
use tauri::Manager;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::storage::{read_json_optional, write_json_atomic};

pub struct AppState {
    pub http: Client,
    layout: Mutex<PayloadLayout>,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    install_state: Mutex<Option<InstallState>>,
    controller_state: Mutex<ControllerState>,
    bridge_instances: Mutex<BridgeInstancesState>,
}

impl AppState {
    pub fn load(app: &tauri::AppHandle) -> ApiResult<Self> {
        let settings_path = app
            .path()
            .resolve("settings.json", BaseDirectory::AppConfig)
            .map_err(|e| ApiError::new("io_path_failed", e.to_string()))?;

        let settings = load_settings(&settings_path)?;
        let layout = PayloadLayout::resolve(settings.payload_root_override.as_deref())?;
        let install_state = load_install_state(&layout, &layout.install_state_file())?;
        let bridge_instances = load_bridge_instances_state(&layout.bridge_instances_file())?;
        let controller_state_raw = load_controller_state(&layout.controller_state_file())?;
        let controller_state = migrate_controller_state(controller_state_raw.clone(), &bridge_instances);
        if controller_state != controller_state_raw {
            let _ = write_json_atomic(&layout.controller_state_file(), &controller_state);
        }

        let http = Client::builder()
            .user_agent("ms-manager")
            .build()
            .map_err(|e| ApiError::new("http_client_failed", e.to_string()))?;
        Ok(Self {
            http,
            layout: Mutex::new(layout),
            settings_path,
            settings: Mutex::new(settings),
            install_state: Mutex::new(install_state),
            controller_state: Mutex::new(controller_state),
            bridge_instances: Mutex::new(bridge_instances),
        })
    }

    pub fn layout_get(&self) -> PayloadLayout {
        self.layout.lock().unwrap().clone()
    }

    pub fn layout_set(&self, next: PayloadLayout) {
        *self.layout.lock().unwrap() = next;
    }

    pub fn payload_state_reload(&self) -> ApiResult<()> {
        let layout = self.layout_get();
        let install_state = load_install_state(&layout, &layout.install_state_file())?;
        let bridge_instances = load_bridge_instances_state(&layout.bridge_instances_file())?;
        let controller_state_raw = load_controller_state(&layout.controller_state_file())?;
        let controller_state = migrate_controller_state(controller_state_raw.clone(), &bridge_instances);
        if controller_state != controller_state_raw {
            let _ = write_json_atomic(&layout.controller_state_file(), &controller_state);
        }
        *self.install_state.lock().unwrap() = install_state;
        *self.bridge_instances.lock().unwrap() = bridge_instances;
        *self.controller_state.lock().unwrap() = controller_state;
        Ok(())
    }

    pub fn settings_set_payload_root_override(
        &self,
        payload_root_override: Option<String>,
    ) -> ApiResult<Settings> {
        let payload_root_override = payload_root_override
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .map(|t| {
                let layout = PayloadLayout::resolve(Some(&t))?;
                Ok::<_, ApiError>(layout.root().display().to_string())
            })
            .transpose()?;

        let mut s = self.settings.lock().unwrap();
        if s.payload_root_override != payload_root_override {
            s.payload_root_override = payload_root_override;
        }
        if s.schema != SETTINGS_SCHEMA {
            s.schema = SETTINGS_SCHEMA;
        }

        write_json_atomic(&self.settings_path, &*s)?;
        Ok(s.clone())
    }

    pub fn install_state_get(&self) -> Option<InstallState> {
        self.install_state.lock().unwrap().clone()
    }

    pub fn install_state_set(&self, next: InstallState) -> ApiResult<InstallState> {
        if next.schema != INSTALL_STATE_SCHEMA {
            return Err(ApiError::new(
                "install_state_schema_invalid",
                format!(
                    "expected schema {INSTALL_STATE_SCHEMA}, got {}",
                    next.schema
                ),
            ));
        }

        let path = self.layout_get().install_state_file();
        write_json_atomic(&path, &next)?;
        *self.install_state.lock().unwrap() = Some(next.clone());
        Ok(next)
    }

    pub fn controller_state_get(&self) -> ControllerState {
        self.controller_state.lock().unwrap().clone()
    }

    pub fn controller_last_flashed_set(
        &self,
        instance_id: &str,
        next: LastFlashed,
    ) -> ApiResult<ControllerState> {
        let mut s = self.controller_state.lock().unwrap();
        s.schema = CONTROLLER_STATE_SCHEMA;
        s.set_last_flashed_for_instance(instance_id.to_string(), next);
        s.last_flashed = None;
        let path = self.layout_get().controller_state_file();
        write_json_atomic(&path, &*s)?;
        Ok(s.clone())
    }

    pub fn bridge_instances_get(&self) -> BridgeInstancesState {
        self.bridge_instances.lock().unwrap().clone()
    }

    pub fn bridge_instances_set(
        &self,
        mut next: BridgeInstancesState,
    ) -> ApiResult<BridgeInstancesState> {
        if next.schema != BRIDGE_INSTANCES_SCHEMA {
            next.schema = BRIDGE_INSTANCES_SCHEMA;
        }
        next.validate().map_err(|reason| {
            ApiError::new("bridge_instances_invalid", reason)
        })?;

        let path = self.layout_get().bridge_instances_file();
        write_json_atomic(&path, &next)?;
        *self.bridge_instances.lock().unwrap() = next.clone();
        Ok(next)
    }

    pub fn bridge_instance_upsert(
        &self,
        next: BridgeInstanceBinding,
    ) -> ApiResult<BridgeInstancesState> {
        let mut state = self.bridge_instances_get();
        state.instances.retain(|instance| instance.instance_id != next.instance_id);
        state.instances.push(next);
        self.bridge_instances_set(state)
    }

    pub fn bridge_instance_remove(&self, instance_id: &str) -> ApiResult<BridgeInstancesState> {
        let mut state = self.bridge_instances_get();
        state.instances.retain(|instance| instance.instance_id != instance_id);
        self.bridge_instances_set(state)
    }

    pub fn bridge_instance_set_target(
        &self,
        instance_id: &str,
        target: FirmwareTarget,
    ) -> ApiResult<BridgeInstancesState> {
        self.update_bridge_instance(instance_id, |instance| {
            instance.target = target;
            Ok(())
        })
    }

    pub fn bridge_instance_set_artifact_source(
        &self,
        instance_id: &str,
        artifact_source: ArtifactSource,
    ) -> ApiResult<BridgeInstancesState> {
        self.update_bridge_instance(instance_id, |instance| {
            instance.artifact_source = artifact_source;
            match artifact_source {
                ArtifactSource::Installed => {
                    if instance.installed_channel.is_none() {
                        instance.installed_channel = Some(Channel::Stable);
                    }
                }
                ArtifactSource::Workspace => {
                    instance.installed_channel = None;
                    instance.installed_pinned_tag = None;
                }
            }
            Ok(())
        })
    }

    pub fn bridge_instance_set_installed_release(
        &self,
        instance_id: &str,
        channel: Channel,
        pinned_tag: Option<String>,
    ) -> ApiResult<BridgeInstancesState> {
        self.update_bridge_instance(instance_id, |instance| {
            instance.artifact_source = ArtifactSource::Installed;
            instance.installed_channel = Some(channel);
            instance.installed_pinned_tag = pinned_tag
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            Ok(())
        })
    }

    pub fn bridge_instance_set_display_name(
        &self,
        instance_id: &str,
        display_name: Option<String>,
    ) -> ApiResult<BridgeInstancesState> {
        self.update_bridge_instance(instance_id, |instance| {
            instance.display_name = display_name
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            Ok(())
        })
    }

    pub fn bridge_instance_set_enabled(
        &self,
        instance_id: &str,
        enabled: bool,
    ) -> ApiResult<BridgeInstancesState> {
        self.update_bridge_instance(instance_id, |instance| {
            instance.enabled = enabled;
            Ok(())
        })
    }

    fn update_bridge_instance<F>(
        &self,
        instance_id: &str,
        update: F,
    ) -> ApiResult<BridgeInstancesState>
    where
        F: FnOnce(&mut BridgeInstanceBinding) -> ApiResult<()>,
    {
        let mut state = self.bridge_instances_get();
        let Some(instance) = state
            .instances
            .iter_mut()
            .find(|instance| instance.instance_id == instance_id)
        else {
            return Err(ApiError::new(
                "bridge_instance_not_found",
                format!("unknown instance_id: {instance_id}"),
            ));
        };

        update(instance)?;
        self.bridge_instances_set(state)
    }
}

fn load_settings(path: &Path) -> ApiResult<Settings> {
    let s = match read_json_optional::<Settings>(path) {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(Settings::default()),
        Err(e) if e.code == "json_parse_failed" => {
            // Keep the app bootable: quarantine the corrupted settings and start fresh.
            let _ = std::fs::rename(path, path.with_extension("corrupt.json"));
            return Ok(Settings::default());
        }
        Err(e) => return Err(e),
    };

    if s.schema != SETTINGS_SCHEMA {
        // Future: add migrations. For now, reset to defaults.
        return Ok(Settings::default());
    }
    Ok(s)
}

fn load_install_state(layout: &PayloadLayout, path: &Path) -> ApiResult<Option<InstallState>> {
    // Migrate legacy filename.
    if !path.exists() {
        let legacy = layout.legacy_install_state_file();
        if legacy.exists() {
            let _ = std::fs::create_dir_all(layout.state_dir());
            let _ = std::fs::rename(&legacy, path);
        }
    }

    let s = match read_json_optional::<InstallState>(path) {
        Ok(v) => v,
        Err(e) if e.code == "json_parse_failed" => {
            // Keep the app bootable: quarantine the corrupted state and start fresh.
            let _ = std::fs::rename(path, path.with_extension("corrupt.json"));
            return Ok(None);
        }
        Err(e) => return Err(e),
    };

    let Some(s) = s else {
        return Ok(None);
    };

    if s.schema != INSTALL_STATE_SCHEMA {
        // Future: add migrations. For now, reset.
        return Ok(None);
    }
    Ok(Some(s))
}

fn load_controller_state(path: &Path) -> ApiResult<ControllerState> {
    let s = match read_json_optional::<ControllerState>(path) {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(ControllerState::default()),
        Err(e) if e.code == "json_parse_failed" => {
            let _ = std::fs::rename(path, path.with_extension("corrupt.json"));
            return Ok(ControllerState::default());
        }
        Err(e) => return Err(e),
    };

    if s.schema != CONTROLLER_STATE_SCHEMA {
        return Ok(ControllerState::default());
    }
    Ok(s)
}

fn load_bridge_instances_state(path: &Path) -> ApiResult<BridgeInstancesState> {
    let s = match read_json_optional::<BridgeInstancesState>(path) {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(BridgeInstancesState::default()),
        Err(e) if e.code == "json_parse_failed" => {
            let _ = std::fs::rename(path, path.with_extension("corrupt.json"));
            return Ok(BridgeInstancesState::default());
        }
        Err(e) => return Err(e),
    };

    if s.schema != BRIDGE_INSTANCES_SCHEMA || s.validate().is_err() {
        return Ok(BridgeInstancesState::default());
    }
    Ok(s)
}

fn migrate_controller_state(
    mut controller_state: ControllerState,
    bridge_instances: &BridgeInstancesState,
) -> ControllerState {
    if controller_state.last_flashed_by_instance.is_empty()
        && controller_state.last_flashed.is_some()
        && bridge_instances.instances.len() == 1
    {
        if let Some(last) = controller_state.last_flashed.clone() {
            controller_state.set_last_flashed_for_instance(
                bridge_instances.instances[0].instance_id.clone(),
                last,
            );
        }
    }

    controller_state.last_flashed = None;
    controller_state
}
