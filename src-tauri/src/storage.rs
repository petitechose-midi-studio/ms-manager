use std::path::Path;

use crate::api_error::{ApiError, ApiResult};

pub fn read_json_optional<T>(path: &Path) -> ApiResult<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = match std::fs::read(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ApiError::new(
                "io_read_failed",
                format!("read {}: {e}", path.display()),
            ))
        }
    };

    serde_json::from_slice(&bytes).map(Some).map_err(|e| {
        ApiError::new(
            "json_parse_failed",
            format!("parse {}: {e}", path.display()),
        )
    })
}

pub fn write_json_atomic<T>(path: &Path, value: &T) -> ApiResult<()>
where
    T: serde::Serialize,
{
    let parent = path.parent().ok_or_else(|| {
        ApiError::new(
            "io_invalid_path",
            format!("no parent for {}", path.display()),
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|e| {
        ApiError::new(
            "io_mkdir_failed",
            format!("create dir {}: {e}", parent.display()),
        )
    })?;

    let tmp = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| ApiError::new("json_serialize_failed", e.to_string()))?;
    std::fs::write(&tmp, bytes)
        .map_err(|e| ApiError::new("io_write_failed", format!("write {}: {e}", tmp.display())))?;

    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        ApiError::new(
            "io_rename_failed",
            format!("rename {} -> {}: {e}", tmp.display(), path.display()),
        )
    })?;
    Ok(())
}
