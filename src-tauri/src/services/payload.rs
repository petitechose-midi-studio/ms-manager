use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::services::install;
use crate::services::process;

#[derive(Debug, Clone)]
pub struct CommandReport {
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn oc_bridge_ctl_shutdown(layout: &PayloadLayout) -> ApiResult<Option<CommandReport>> {
    let exe = oc_bridge_path(layout);
    if !exe.exists() {
        return Ok(None);
    }

    let mut cmd = Command::new(&exe);
    process::no_console_window(&mut cmd);
    let out = cmd
        .args(["ctl", "shutdown"])
        .output()
        .await
        .map_err(|e| ApiError::new("io_exec_failed", format!("oc-bridge ctl shutdown: {e}")))?;

    Ok(Some(make_report(out)))
}

pub async fn oc_bridge_kill_process() -> ApiResult<Option<CommandReport>> {
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        process::no_console_window(&mut cmd);
        let out = cmd
            .args(["/IM", "oc-bridge.exe", "/T", "/F"])
            .output()
            .await
            .map_err(|e| ApiError::new("io_exec_failed", format!("taskkill: {e}")))?;

        let mut r = make_report(out);
        // taskkill returns 128 when the process is not found.
        if r.exit_code == Some(128) {
            r.ok = true;
        }
        return Ok(Some(r));
    }

    #[cfg(unix)]
    {
        let mut cmd = Command::new("pkill");
        process::no_console_window(&mut cmd);
        let out = cmd
            .args(["-x", "oc-bridge"])
            .output()
            .await
            .map_err(|e| ApiError::new("io_exec_failed", format!("pkill: {e}")))?;

        let mut r = make_report(out);
        // pkill returns 1 when no processes were matched.
        if r.exit_code == Some(1) {
            r.ok = true;
        }
        return Ok(Some(r));
    }
}

pub async fn relocate_payload_root(
    old_layout: PayloadLayout,
    new_layout: PayloadLayout,
    installed_tag: Option<String>,
) -> ApiResult<()> {
    tokio::task::spawn_blocking(move || {
        relocate_payload_root_blocking(&old_layout, &new_layout, installed_tag.as_deref())
    })
    .await
    .map_err(|e| ApiError::new("internal_error", format!("relocate join: {e}")))??;
    Ok(())
}

fn relocate_payload_root_blocking(
    old_layout: &PayloadLayout,
    new_layout: &PayloadLayout,
    installed_tag: Option<&str>,
) -> ApiResult<()> {
    let old_root = old_layout.root();
    let new_root = new_layout.root();

    if new_root.starts_with(old_root) || old_root.starts_with(new_root) {
        return Err(ApiError::new(
            "payload_root_invalid",
            "payload_root_override must not be nested inside the current root",
        )
        .with_details(serde_json::json!({
            "old_root": old_root.display().to_string(),
            "new_root": new_root.display().to_string(),
        })));
    }

    if new_root.exists() {
        // If the folder exists, only accept it when empty.
        let read = std::fs::read_dir(new_root).map_err(|e| {
            ApiError::new(
                "io_read_failed",
                format!("read dir {}: {e}", new_root.display()),
            )
        })?;
        if read.into_iter().next().is_some() {
            return Err(ApiError::new(
                "payload_root_exists",
                "target payload root already exists and is not empty",
            )
            .with_details(serde_json::json!({
                "new_root": new_root.display().to_string(),
            })));
        }

        // Allow rename to a "pre-created" destination dir.
        // (std::fs::rename requires destination to not exist.)
        std::fs::remove_dir(new_root).map_err(|e| {
            ApiError::new(
                "io_remove_failed",
                format!("remove {}: {e}", new_root.display()),
            )
        })?;
    } else if let Some(parent) = new_root.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ApiError::new(
                "io_mkdir_failed",
                format!("create dir {}: {e}", parent.display()),
            )
        })?;
    }

    match try_rename_with_retry(old_root, new_root) {
        Ok(()) => {
            if let Some(tag) = installed_tag {
                install::set_current(new_layout, tag)?;
            }
            return Ok(());
        }
        Err(e) if is_cross_device(&e) || is_access_denied(&e) => {
            // Fall back to copy+swap. This is also used on Windows when the directory is locked.
            copy_swap_payload_root(old_layout, new_layout, installed_tag)?;
            return Ok(());
        }
        Err(e) => {
            return Err(ApiError::new(
                "payload_root_move_failed",
                format!("move {} -> {}: {e}", old_root.display(), new_root.display()),
            ));
        }
    }
}

fn copy_swap_payload_root(
    old_layout: &PayloadLayout,
    new_layout: &PayloadLayout,
    installed_tag: Option<&str>,
) -> ApiResult<()> {
    let old_root = old_layout.root();
    let new_root = new_layout.root();

    let staging = new_root.with_extension("staging");
    if staging.exists() {
        return Err(
            ApiError::new("payload_root_exists", "staging directory already exists").with_details(
                serde_json::json!({
                    "staging": staging.display().to_string(),
                }),
            ),
        );
    }

    std::fs::create_dir_all(&staging).map_err(|e| {
        ApiError::new(
            "io_mkdir_failed",
            format!("create dir {}: {e}", staging.display()),
        )
    })?;

    let copy_res = (|| {
        for name in ["versions", "cache", "state"] {
            let src = old_root.join(name);
            if !src.exists() {
                continue;
            }
            let dst = staging.join(name);
            copy_dir_recursive(&src, &dst)?;
        }
        Ok::<_, ApiError>(())
    })();

    if let Err(e) = copy_res {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    // Replace destination atomically.
    if new_root.exists() {
        let _ = std::fs::remove_dir(new_root);
    }
    std::fs::rename(&staging, new_root).map_err(|e| {
        ApiError::new(
            "payload_root_move_failed",
            format!("move {} -> {}: {e}", staging.display(), new_root.display()),
        )
    })?;

    if let Some(tag) = installed_tag {
        install::set_current(new_layout, tag)?;
    }

    // Best-effort cleanup.
    let _ = remove_current_pointer(old_layout);
    let _ = std::fs::remove_dir_all(old_root.join("versions"));
    let _ = std::fs::remove_dir_all(old_root.join("cache"));
    let _ = std::fs::remove_dir_all(old_root.join("state"));
    let _ = std::fs::remove_dir(old_root);

    Ok(())
}

fn try_rename_with_retry(src: &Path, dst: &Path) -> Result<(), std::io::Error> {
    let delays_ms = [50u64, 150, 400, 900];
    for (i, delay) in delays_ms.iter().enumerate() {
        match std::fs::rename(src, dst) {
            Ok(()) => return Ok(()),
            Err(e) => {
                if i + 1 == delays_ms.len() {
                    return Err(e);
                }
                if !is_access_denied(&e) {
                    return Err(e);
                }
                std::thread::sleep(std::time::Duration::from_millis(*delay));
            }
        }
    }
    // unreachable
    std::fs::rename(src, dst)
}

fn is_access_denied(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(5)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> ApiResult<()> {
    std::fs::create_dir_all(dst).map_err(|e| {
        ApiError::new(
            "io_mkdir_failed",
            format!("create dir {}: {e}", dst.display()),
        )
    })?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| ApiError::new("io_read_failed", format!("read dir {}: {e}", src.display())))?
    {
        let entry = entry.map_err(|e| ApiError::new("io_read_failed", e.to_string()))?;
        let ft = entry
            .file_type()
            .map_err(|e| ApiError::new("io_read_failed", e.to_string()))?;
        let sp = entry.path();
        let dp = dst.join(entry.file_name());

        if ft.is_dir() {
            copy_dir_recursive(&sp, &dp)?;
            continue;
        }

        if ft.is_file() {
            if let Some(parent) = dp.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::copy(&sp, &dp).map_err(|e| {
                ApiError::new(
                    "io_copy_failed",
                    format!("copy {} -> {}: {e}", sp.display(), dp.display()),
                )
            })?;
        }
    }

    Ok(())
}

fn remove_current_pointer(layout: &PayloadLayout) -> ApiResult<()> {
    let current = layout.current_dir();
    if std::fs::symlink_metadata(&current).is_err() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        std::fs::remove_dir(&current).map_err(|e| {
            ApiError::new(
                "io_remove_failed",
                format!("remove {}: {e}", current.display()),
            )
        })?;
        return Ok(());
    }

    #[cfg(unix)]
    {
        if std::fs::remove_file(&current).is_err() {
            let _ = std::fs::remove_dir_all(&current);
        }
        Ok(())
    }
}

fn make_report(out: std::process::Output) -> CommandReport {
    CommandReport {
        ok: out.status.success(),
        exit_code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

pub(crate) fn oc_bridge_path(layout: &PayloadLayout) -> PathBuf {
    let bin = layout.current_dir().join("bin");
    if cfg!(windows) {
        bin.join("oc-bridge.exe")
    } else {
        bin.join("oc-bridge")
    }
}

fn is_cross_device(e: &std::io::Error) -> bool {
    // Unix: EXDEV (18) - cross-device link
    // Windows: ERROR_NOT_SAME_DEVICE (17)
    matches!(e.raw_os_error(), Some(18) | Some(17))
}
