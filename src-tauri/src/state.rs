use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ms_manager_core::{Channel, InstallState, Settings, INSTALL_STATE_SCHEMA, SETTINGS_SCHEMA};
use reqwest::Client;
use tauri::path::BaseDirectory;
use tauri::Manager;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::storage::{read_json_optional, write_json_atomic};

pub struct AppState {
    pub http: Client,
    pub layout: PayloadLayout,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    install_state_path: PathBuf,
    install_state: Mutex<Option<InstallState>>,
}

impl AppState {
    pub fn load(app: &tauri::AppHandle) -> ApiResult<Self> {
        let layout = PayloadLayout::resolve()?;

        let settings_path = app
            .path()
            .resolve("settings.json", BaseDirectory::AppConfig)
            .map_err(|e| ApiError::new("io_path_failed", e.to_string()))?;

        let settings = load_settings(&settings_path)?;

        let install_state_path = layout.state_file();
        let install_state = load_install_state(&install_state_path)?;

        let http = Client::builder()
            .user_agent("ms-manager")
            .build()
            .map_err(|e| ApiError::new("http_client_failed", e.to_string()))?;

        Ok(Self {
            http,
            layout,
            settings_path,
            settings: Mutex::new(settings),
            install_state_path,
            install_state: Mutex::new(install_state),
        })
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

        write_json_atomic(&self.install_state_path, &next)?;
        *self.install_state.lock().unwrap() = Some(next.clone());
        Ok(next)
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

fn load_install_state(path: &Path) -> ApiResult<Option<InstallState>> {
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
