use std::path::{Path, PathBuf};

use crate::api_error::{ApiError, ApiResult};

#[derive(Debug, Clone)]
pub struct PayloadLayout {
    root: PathBuf,
}

impl PayloadLayout {
    pub fn resolve(payload_root_override: Option<&str>) -> ApiResult<Self> {
        let root = match payload_root_override
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(v) => parse_payload_root_override(v)?,
            None => payload_root()?,
        };

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }

    pub fn current_dir(&self) -> PathBuf {
        self.root.join("current")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn install_state_file(&self) -> PathBuf {
        self.state_dir().join("install_state.json")
    }

    pub fn legacy_install_state_file(&self) -> PathBuf {
        // Pre-2026-02-04 legacy name.
        self.state_dir().join("state.json")
    }

    pub fn controller_state_file(&self) -> PathBuf {
        self.state_dir().join("controller.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn asset_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("assets")
    }

    pub fn asset_cache_path(&self, sha256: &str, filename: &str) -> PathBuf {
        // Use sha256 as a stable content address.
        self.asset_cache_dir().join(sha256).join(filename)
    }

    pub fn version_dir(&self, tag: &str) -> PathBuf {
        self.versions_dir().join(tag)
    }

    pub fn version_staging_dir(&self, tag: &str) -> PathBuf {
        self.versions_dir().join(format!("{tag}.staging"))
    }
}

fn parse_payload_root_override(s: &str) -> ApiResult<PathBuf> {
    // Basic normalization; keep it predictable and platform-native.
    let p = if cfg!(windows) {
        PathBuf::from(s)
    } else if s == "~" {
        let home =
            std::env::var_os("HOME").ok_or_else(|| ApiError::new("env_missing", "missing HOME"))?;
        PathBuf::from(home)
    } else if let Some(rest) = s.strip_prefix("~/") {
        let home =
            std::env::var_os("HOME").ok_or_else(|| ApiError::new("env_missing", "missing HOME"))?;
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(s)
    };

    if !p.is_absolute() {
        return Err(ApiError::new(
            "payload_root_invalid",
            "payload_root_override must be an absolute path",
        )
        .with_details(serde_json::json!({"value": s})));
    }

    Ok(p)
}

fn payload_root() -> ApiResult<PathBuf> {
    match std::env::consts::OS {
        "windows" => {
            let base = std::env::var_os("PROGRAMDATA")
                .ok_or_else(|| ApiError::new("env_missing", "missing PROGRAMDATA"))?;
            Ok(PathBuf::from(base).join("MIDI Studio"))
        }
        "macos" => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| ApiError::new("env_missing", "missing HOME"))?;
            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("MIDI Studio"))
        }
        "linux" => {
            if let Some(v) = std::env::var_os("XDG_DATA_HOME") {
                return Ok(PathBuf::from(v).join("midi-studio"));
            }

            let home = std::env::var_os("HOME")
                .ok_or_else(|| ApiError::new("env_missing", "missing HOME"))?;
            Ok(PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("midi-studio"))
        }
        other => Err(ApiError::new(
            "unsupported_platform",
            format!("unsupported platform: {other}"),
        )),
    }
}
