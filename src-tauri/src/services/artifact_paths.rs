use std::path::{Path, PathBuf};

use crate::api_error::{ApiError, ApiResult};

pub fn dev_artifacts_path() -> PathBuf {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri should have a repo root parent")
        .to_path_buf();
    dev_artifacts_path_from_root(&repo_root)
}

fn dev_artifacts_path_from_root(repo_root: &Path) -> PathBuf {
    let local = repo_root.join("dev-artifacts.local.json");
    if local.exists() {
        return local;
    }

    repo_root.join("dev-artifacts.json")
}

pub fn ensure_file_exists(key: &str, path: &Path) -> ApiResult<()> {
    if path.exists() && path.is_file() {
        return Ok(());
    }

    let message = missing_artifact_message(key, path);

    Err(
        ApiError::new("artifact_missing", message).with_details(serde_json::json!({
            "artifact": key,
            "path": path.display().to_string(),
        })),
    )
}

fn missing_artifact_message(key: &str, path: &Path) -> String {
    let base = format!("artifact '{key}' not found: {}", path.display());

    if path.extension().and_then(|ext| ext.to_str()) != Some("hex") {
        return base;
    }

    let Some(parent) = path.parent() else {
        return base;
    };

    let idedata = parent.join("idedata.json");
    let elf = parent.join("firmware.elf");

    if idedata.exists() && elf.exists() {
        return format!(
            "{base} (PlatformIO metadata and firmware.elf are present, but midi-studio-loader requires an Intel HEX file)"
        );
    }

    if idedata.exists() {
        return format!(
            "{base} (PlatformIO environment exists, but the HEX artifact was not generated)"
        );
    }

    base
}

pub fn ui_path_string(path: &Path) -> String {
    let raw = path.display().to_string();
    strip_windows_verbatim_prefix(&raw)
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    #[cfg(windows)]
    {
        if let Some(rest) = path.strip_prefix("\\\\?\\UNC\\") {
            return format!("\\\\{rest}");
        }
        if let Some(rest) = path.strip_prefix("\\\\?\\") {
            return rest.to_string();
        }
    }

    path.to_string()
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
    fn generated_config_is_the_default() {
        let root = unique_test_dir("generated-artifact-path");
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(
            dev_artifacts_path_from_root(&root),
            root.join("dev-artifacts.json")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_config_overrides_generated_config() {
        let root = unique_test_dir("local-artifact-path");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("dev-artifacts.json"), "{}").unwrap();
        std::fs::write(root.join("dev-artifacts.local.json"), "{}").unwrap();

        assert_eq!(
            dev_artifacts_path_from_root(&root),
            root.join("dev-artifacts.local.json")
        );

        std::fs::remove_dir_all(root).unwrap();
    }
}
