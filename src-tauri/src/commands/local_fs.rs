use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api_error::{ApiError, ApiResult};

#[derive(Debug, Clone, Deserialize)]
pub struct LocalFsListRequest {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalFsPathRequest {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalFsDeleteRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalFsRenameRequest {
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalFsFileType {
    File,
    Directory,
    Other,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalFsEntry {
    pub name: String,
    pub path: String,
    pub file_type: LocalFsFileType,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalFsListResponse {
    pub root_path: String,
    pub path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<LocalFsEntry>,
}

#[tauri::command]
pub fn local_fs_list(request: LocalFsListRequest) -> ApiResult<LocalFsListResponse> {
    let root = ensure_local_storage_root()?;
    let relative_parts = normalize_local_relative_parts(request.path.as_deref());
    let relative_path = parts_to_posix_path(&relative_parts);
    let path = root.join(parts_to_native_path(&relative_parts));
    let path = path
        .canonicalize()
        .map_err(|err| io_error("resolve local folder", &path, err))?;
    ensure_inside_root(&root, &path)?;
    if !path.is_dir() {
        return Err(ApiError::new(
            "local_fs_not_directory",
            format!("local path is not a directory: {relative_path}"),
        ));
    }

    let mut entries = Vec::new();
    for entry in
        std::fs::read_dir(&path).map_err(|err| io_error("list local folder", &path, err))?
    {
        let entry = entry.map_err(|err| io_error("read local folder entry", &path, err))?;
        let entry_path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = entry
            .metadata()
            .map_err(|err| io_error("read local entry metadata", &entry_path, err))?;
        let entry_relative_path = join_posix_child(&relative_path, &name);
        let file_type = if metadata.is_dir() {
            LocalFsFileType::Directory
        } else if metadata.is_file() {
            LocalFsFileType::File
        } else {
            LocalFsFileType::Other
        };
        let size_bytes = metadata.is_file().then_some(metadata.len());
        entries.push(LocalFsEntry {
            name,
            path: entry_relative_path,
            file_type,
            size_bytes,
        });
    }

    entries.sort_by(|a, b| {
        let a_is_dir = matches!(a.file_type, LocalFsFileType::Directory);
        let b_is_dir = matches!(b.file_type, LocalFsFileType::Directory);
        b_is_dir
            .cmp(&a_is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(LocalFsListResponse {
        root_path: root.display().to_string(),
        parent_path: parent_posix_path(&relative_parts),
        path: relative_path,
        entries,
    })
}

#[tauri::command]
pub fn local_fs_mkdir(request: LocalFsPathRequest) -> ApiResult<()> {
    let path = resolve_local_storage_path(&request.path)?;
    std::fs::create_dir_all(&path).map_err(|err| io_error("create local folder", &path, err))
}

#[tauri::command]
pub fn local_fs_delete(request: LocalFsDeleteRequest) -> ApiResult<()> {
    let root = ensure_local_storage_root()?;
    let path = resolve_local_storage_path(&request.path)?;
    ensure_not_root(&root, &path, "delete")?;

    let metadata = std::fs::metadata(&path)
        .map_err(|err| io_error("read local delete metadata", &path, err))?;
    if metadata.is_dir() {
        if request.recursive {
            std::fs::remove_dir_all(&path)
                .map_err(|err| io_error("delete local folder", &path, err))
        } else {
            std::fs::remove_dir(&path).map_err(|err| io_error("delete local folder", &path, err))
        }
    } else {
        std::fs::remove_file(&path).map_err(|err| io_error("delete local file", &path, err))
    }
}

#[tauri::command]
pub fn local_fs_rename(request: LocalFsRenameRequest) -> ApiResult<()> {
    let root = ensure_local_storage_root()?;
    let from_path = resolve_local_storage_path(&request.from_path)?;
    let to_path = resolve_local_storage_path(&request.to_path)?;
    ensure_not_root(&root, &from_path, "rename")?;

    if to_path.exists() {
        return Err(ApiError::new(
            "local_fs_destination_exists",
            format!("local destination already exists: {}", to_path.display()),
        ));
    }

    std::fs::rename(&from_path, &to_path).map_err(|err| {
        ApiError::new(
            "local_fs_io_failed",
            format!(
                "rename local entry: {} -> {}: {err}",
                from_path.display(),
                to_path.display()
            ),
        )
    })
}

pub(crate) fn resolve_local_storage_path(path: &str) -> ApiResult<PathBuf> {
    let root = ensure_local_storage_root()?;
    let parts = normalize_local_relative_parts(Some(path));
    let path = root.join(parts_to_native_path(&parts));
    let resolved = if path.exists() {
        path.canonicalize()
            .map_err(|err| io_error("resolve local storage path", &path, err))?
    } else {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(&root);
        let resolved_parent = parent
            .canonicalize()
            .map_err(|err| io_error("resolve local storage parent", parent, err))?;
        ensure_inside_root(&root, &resolved_parent)?;
        return Ok(resolved_parent.join(
            path.file_name()
                .ok_or_else(|| ApiError::new("local_fs_path_invalid", "local path is empty"))?,
        ));
    };
    ensure_inside_root(&root, &resolved)?;
    Ok(resolved)
}

pub(crate) fn ensure_local_storage_root() -> ApiResult<PathBuf> {
    let root = local_storage_root()?;
    std::fs::create_dir_all(&root)
        .map_err(|err| io_error("create local storage root", &root, err))?;
    root.canonicalize()
        .map_err(|err| io_error("resolve local storage root", &root, err))
}

fn local_storage_root() -> ApiResult<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::new("env_missing", "USERPROFILE/HOME is not set"))?;

    let documents = home.join("Documents");
    if documents.is_dir() || !documents.exists() {
        return Ok(documents.join("MIDI Studio").join("Storage"));
    }

    Ok(home.join("MIDI Studio").join("Storage"))
}

fn normalize_local_relative_parts(path: Option<&str>) -> Vec<String> {
    let value = path.unwrap_or("/").trim().replace('\\', "/");
    let mut parts = Vec::new();

    for part in value.split('/') {
        match part.trim() {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            clean => parts.push(clean.to_string()),
        }
    }

    parts
}

fn parts_to_native_path(parts: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for part in parts {
        path.push(part);
    }
    path
}

fn parts_to_posix_path(parts: &[String]) -> String {
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn parent_posix_path(parts: &[String]) -> Option<String> {
    if parts.is_empty() {
        return None;
    }

    Some(parts_to_posix_path(&parts[..parts.len() - 1]))
}

fn join_posix_child(base: &str, name: &str) -> String {
    if base == "/" {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn ensure_inside_root(root: &Path, path: &Path) -> ApiResult<()> {
    if path.starts_with(root) {
        return Ok(());
    }

    Err(ApiError::new(
        "local_fs_path_outside_root",
        format!(
            "local path escapes MIDI Studio storage root: {}",
            path.display()
        ),
    ))
}

fn ensure_not_root(root: &Path, path: &Path, action: &str) -> ApiResult<()> {
    if path == root {
        return Err(ApiError::new(
            "local_fs_root_protected",
            format!("cannot {action} the MIDI Studio storage root"),
        ));
    }
    Ok(())
}

fn io_error(action: &str, path: impl AsRef<Path>, err: std::io::Error) -> ApiError {
    ApiError::new(
        "local_fs_io_failed",
        format!("{action}: {}: {err}", path.as_ref().display()),
    )
}
