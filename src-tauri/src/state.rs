use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ms_manager_core::{Channel, Settings, SETTINGS_SCHEMA};
use reqwest::Client;
use tauri::path::BaseDirectory;
use tauri::Manager;

use crate::api_error::{ApiError, ApiResult};
use crate::storage::{read_json_optional, write_json_atomic};

pub struct AppState {
    pub http: Client,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
}

impl AppState {
    pub fn load(app: &tauri::AppHandle) -> ApiResult<Self> {
        let settings_path = app
            .path()
            .resolve("settings.json", BaseDirectory::AppConfig)
            .map_err(|e| ApiError::new("io_path_failed", e.to_string()))?;

        let settings = load_settings(&settings_path)?;

        let http = Client::builder()
            .user_agent("ms-manager")
            .build()
            .map_err(|e| ApiError::new("http_client_failed", e.to_string()))?;

        Ok(Self {
            http,
            settings_path,
            settings: Mutex::new(settings),
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
