use std::path::{Path, PathBuf};

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::models::InstallPlan;
use crate::services::assets::CachedAsset;
use crate::services::process;

#[derive(Debug, Clone)]
pub struct InstalledVersion {
    pub tag: String,
    pub profile: String,
}

pub async fn apply_install(
    layout: &PayloadLayout,
    plan: &InstallPlan,
    cached: &[CachedAsset],
) -> ApiResult<InstalledVersion> {
    let versions_dir = layout.versions_dir();
    std::fs::create_dir_all(&versions_dir).map_err(|e| {
        ApiError::new(
            "io_mkdir_failed",
            format!("create dir {}: {e}", versions_dir.display()),
        )
    })?;

    let version_dir = layout.version_dir(&plan.tag);
    if !version_dir.exists() {
        install_fresh(layout, plan, cached).await?;
    } else {
        install_additional_assets(&version_dir, cached)?;
        ensure_bundle_executables(&version_dir)?;
    }

    set_current(layout, &plan.tag)?;

    Ok(InstalledVersion {
        tag: plan.tag.clone(),
        profile: plan.profile.clone(),
    })
}

async fn install_fresh(
    layout: &PayloadLayout,
    plan: &InstallPlan,
    cached: &[CachedAsset],
) -> ApiResult<()> {
    let staging_dir = layout.version_staging_dir(&plan.tag);
    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir).map_err(|e| {
            ApiError::new(
                "io_remove_failed",
                format!("remove {}: {e}", staging_dir.display()),
            )
        })?;
    }

    std::fs::create_dir_all(&staging_dir).map_err(|e| {
        ApiError::new(
            "io_mkdir_failed",
            format!("create dir {}: {e}", staging_dir.display()),
        )
    })?;

    let bundle = cached
        .iter()
        .find(|a| a.plan.kind == "bundle")
        .ok_or_else(|| ApiError::new("install_plan_invalid", "missing bundle asset"))?;

    extract_zip_into(&bundle.path, &staging_dir).await?;
    install_additional_assets(&staging_dir, cached)?;
    ensure_bundle_executables(&staging_dir)?;

    let version_dir = layout.version_dir(&plan.tag);
    if version_dir.exists() {
        // Another process installed it while we were staging.
        std::fs::remove_dir_all(&staging_dir).ok();
        return Ok(());
    }

    std::fs::rename(&staging_dir, &version_dir).map_err(|e| {
        ApiError::new(
            "io_rename_failed",
            format!(
                "rename {} -> {}: {e}",
                staging_dir.display(),
                version_dir.display()
            ),
        )
    })?;

    Ok(())
}

fn install_additional_assets(version_dir: &Path, cached: &[CachedAsset]) -> ApiResult<()> {
    for a in cached {
        if a.plan.kind == "bundle" {
            continue;
        }

        let rel = asset_relative_path(&a.plan)?;
        let dest = version_dir.join(rel);
        let parent = dest.parent().ok_or_else(|| {
            ApiError::new(
                "io_invalid_path",
                format!("no parent for {}", dest.display()),
            )
        })?;
        std::fs::create_dir_all(parent).map_err(|e| {
            ApiError::new(
                "io_mkdir_failed",
                format!("create dir {}: {e}", parent.display()),
            )
        })?;

        std::fs::copy(&a.path, &dest).map_err(|e| {
            ApiError::new(
                "io_copy_failed",
                format!("copy {} -> {}: {e}", a.path.display(), dest.display()),
            )
        })?;
    }

    Ok(())
}

fn asset_relative_path(asset: &crate::models::AssetPlan) -> ApiResult<PathBuf> {
    if asset.filename.is_empty() {
        return Err(ApiError::new("asset_invalid", "missing filename"));
    }

    match asset.kind.as_str() {
        "firmware" => Ok(PathBuf::from("firmware").join(&asset.filename)),
        "bitwig-extension" => Ok(PathBuf::from("integrations")
            .join("bitwig")
            .join(&asset.filename)),
        other => Ok(PathBuf::from("assets").join(other).join(&asset.filename)),
    }
}

async fn extract_zip_into(zip_path: &Path, dest_dir: &Path) -> ApiResult<()> {
    let zip_path = zip_path.to_path_buf();
    let dest_dir = dest_dir.to_path_buf();

    tauri::async_runtime::spawn_blocking(move || extract_zip_into_blocking(&zip_path, &dest_dir))
        .await
        .map_err(|e| ApiError::new("internal_error", format!("extract task failed: {e}")))??;
    Ok(())
}

fn extract_zip_into_blocking(zip_path: &Path, dest_dir: &Path) -> ApiResult<()> {
    let f = std::fs::File::open(zip_path).map_err(|e| {
        ApiError::new(
            "io_read_failed",
            format!("open {}: {e}", zip_path.display()),
        )
    })?;

    let mut archive = zip::ZipArchive::new(f)
        .map_err(|e| ApiError::new("zip_invalid", format!("open zip: {e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ApiError::new("zip_invalid", format!("zip entry {i}: {e}")))?;

        let out_rel = entry
            .enclosed_name()
            .ok_or_else(|| ApiError::new("zip_invalid", "zip contains invalid path"))?
            .to_path_buf();
        let out_path = dest_dir.join(out_rel);

        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&out_path).map_err(|e| {
                ApiError::new(
                    "io_mkdir_failed",
                    format!("create dir {}: {e}", out_path.display()),
                )
            })?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ApiError::new(
                    "io_mkdir_failed",
                    format!("create dir {}: {e}", parent.display()),
                )
            })?;
        }

        let mut out = std::fs::File::create(&out_path).map_err(|e| {
            ApiError::new(
                "io_write_failed",
                format!("create {}: {e}", out_path.display()),
            )
        })?;

        std::io::copy(&mut entry, &mut out).map_err(|e| {
            ApiError::new(
                "io_write_failed",
                format!("extract {}: {e}", out_path.display()),
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(mode));
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn ensure_bundle_executables(version_dir: &Path) -> ApiResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let candidates = ["bin/oc-bridge", "bin/midi-studio-loader"];
    for rel in candidates {
        let p = version_dir.join(rel);
        if !p.exists() {
            continue;
        }
        let mut perm = std::fs::metadata(&p)
            .map_err(|e| ApiError::new("io_read_failed", format!("stat {}: {e}", p.display())))?
            .permissions();
        let mode = perm.mode();
        perm.set_mode(mode | 0o111);
        std::fs::set_permissions(&p, perm)
            .map_err(|e| ApiError::new("io_write_failed", format!("chmod {}: {e}", p.display())))?;
    }

    Ok(())
}

#[cfg(not(unix))]
fn ensure_bundle_executables(_version_dir: &Path) -> ApiResult<()> {
    Ok(())
}

pub(crate) fn set_current(layout: &PayloadLayout, tag: &str) -> ApiResult<()> {
    let current = layout.current_dir();
    let target = layout.version_dir(tag);
    if !target.exists() {
        return Err(ApiError::new(
            "install_missing_version",
            format!("missing installed version dir: {}", target.display()),
        ));
    }

    if std::fs::symlink_metadata(&current).is_ok() {
        #[cfg(windows)]
        {
            // `current` is expected to be a junction. Use remove_dir (NOT remove_dir_all) to avoid
            // any risk of following the reparse point.
            std::fs::remove_dir(&current).map_err(|e| {
                ApiError::new(
                    "io_remove_failed",
                    format!("remove {}: {e}", current.display()),
                )
            })?;
        }

        #[cfg(unix)]
        {
            // Symlink removal.
            if std::fs::remove_file(&current).is_err() {
                let _ = std::fs::remove_dir_all(&current);
            }
        }
    }

    #[cfg(windows)]
    {
        // Prefer a junction for a stable, non-admin `current/` pointer.
        let mut cmd = std::process::Command::new("cmd");
        process::no_console_window_std(&mut cmd);
        let out = cmd
            .args(["/c", "mklink", "/J"])
            .arg(&current)
            .arg(&target)
            .output()
            .map_err(|e| ApiError::new("io_exec_failed", format!("mklink: {e}")))?;

        if !out.status.success() {
            return Err(
                ApiError::new("current_link_failed", "failed to create current junction")
                    .with_details(serde_json::json!({
                        "exit_code": out.status.code(),
                        "stdout": String::from_utf8_lossy(&out.stdout).trim(),
                        "stderr": String::from_utf8_lossy(&out.stderr).trim(),
                        "current": current.display().to_string(),
                        "target": target.display().to_string(),
                    })),
            );
        }
    }

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &current).map_err(|e| {
            ApiError::new(
                "current_link_failed",
                format!("symlink {} -> {}: {e}", current.display(), target.display()),
            )
        })?;
    }

    Ok(())
}
