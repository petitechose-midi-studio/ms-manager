use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ms_manager_core::{
    Channel, ControllerState, InstallState, LastFlashed, Settings, CONTROLLER_STATE_SCHEMA,
    INSTALL_STATE_SCHEMA, SETTINGS_SCHEMA,
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
        let controller_state = load_controller_state(&layout.controller_state_file())?;

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
        let controller_state = load_controller_state(&layout.controller_state_file())?;
        *self.install_state.lock().unwrap() = install_state;
        *self.controller_state.lock().unwrap() = controller_state;
        Ok(())
    }

    pub fn settings_get(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn settings_set_channel(&self, channel: Channel) -> ApiResult<Settings> {
        let mut s = self.settings.lock().unwrap();
        if s.channel != channel {
            s.channel = channel;
            s.pinned_tag = None;
        }
        if s.schema != SETTINGS_SCHEMA {
            s.schema = SETTINGS_SCHEMA;
        }

        write_json_atomic(&self.settings_path, &*s)?;
        Ok(s.clone())
    }

    pub fn settings_set_profile(&self, profile: String) -> ApiResult<Settings> {
        let profile = profile.trim().to_string();
        if profile.is_empty() {
            return Err(ApiError::new("invalid_profile", "profile cannot be empty"));
        }

        let mut s = self.settings.lock().unwrap();
        if s.profile != profile {
            s.profile = profile;
        }
        if s.schema != SETTINGS_SCHEMA {
            s.schema = SETTINGS_SCHEMA;
        }

        write_json_atomic(&self.settings_path, &*s)?;
        Ok(s.clone())
    }

    pub fn settings_set_pinned_tag(&self, pinned_tag: Option<String>) -> ApiResult<Settings> {
        let pinned_tag = pinned_tag
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        let mut s = self.settings.lock().unwrap();
        if s.pinned_tag != pinned_tag {
            s.pinned_tag = pinned_tag;
        }
        if s.schema != SETTINGS_SCHEMA {
            s.schema = SETTINGS_SCHEMA;
        }

        write_json_atomic(&self.settings_path, &*s)?;
        Ok(s.clone())
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

    pub fn controller_last_flashed(&self) -> Option<LastFlashed> {
        self.controller_state.lock().unwrap().last_flashed.clone()
    }

    pub fn controller_last_flashed_set(&self, next: LastFlashed) -> ApiResult<ControllerState> {
        let mut s = self.controller_state.lock().unwrap();
        s.schema = CONTROLLER_STATE_SCHEMA;
        s.last_flashed = Some(next);
        let path = self.layout_get().controller_state_file();
        write_json_atomic(&path, &*s)?;
        Ok(s.clone())
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
