use std::path::{Path, PathBuf};

use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::models::AssetPlan;

#[derive(Debug, Clone)]
pub struct CachedAsset {
    pub plan: AssetPlan,
    pub path: PathBuf,
}

pub async fn ensure_asset_cached(
    client: &reqwest::Client,
    layout: &PayloadLayout,
    asset: &AssetPlan,
) -> ApiResult<PathBuf> {
    if asset.sha256.is_empty() {
        return Err(ApiError::new("asset_invalid", "missing sha256"));
    }
    if asset.filename.is_empty() {
        return Err(ApiError::new("asset_invalid", "missing filename"));
    }
    if asset.url.is_empty() {
        return Err(ApiError::new("asset_invalid", "missing url"));
    }

    let dest = layout.asset_cache_path(&asset.sha256, &asset.filename);

    // Fast path: already cached.
    if dest.exists() {
        if let Ok(meta) = std::fs::metadata(&dest) {
            if meta.len() == asset.size {
                let got = sha256_file_hex(&dest)?;
                if got == asset.sha256 {
                    return Ok(dest);
                }
            }
        }

        // Corrupt cache entry; remove and re-download.
        let _ = std::fs::remove_file(&dest);
    }

    let parent = dest
        .parent()
        .ok_or_else(|| ApiError::new("io_invalid_path", format!("no parent for {}", dest.display())))?;
    std::fs::create_dir_all(parent).map_err(|e| {
        ApiError::new(
            "io_mkdir_failed",
            format!("create dir {}: {e}", parent.display()),
        )
    })?;

    let tmp = dest.with_extension("download");
    if tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
    }

    download_verify_to_file(client, &asset.url, asset.size, &asset.sha256, &tmp).await?;

    // Best-effort: remove any previous file.
    if dest.exists() {
        let _ = std::fs::remove_file(&dest);
    }
    std::fs::rename(&tmp, &dest).map_err(|e| {
        ApiError::new(
            "io_rename_failed",
            format!("rename {} -> {}: {e}", tmp.display(), dest.display()),
        )
    })?;

    Ok(dest)
}

async fn download_verify_to_file(
    client: &reqwest::Client,
    url: &str,
    expected_size: u64,
    expected_sha256: &str,
    dest: &Path,
) -> ApiResult<()> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| ApiError::new("http_request_failed", format!("GET {url}: {e}")))?;

    let status = res.status();
    if !status.is_success() {
        return Err(http_status_error(url, status));
    }

    let mut file = tokio::fs::File::create(dest).await.map_err(|e| {
        ApiError::new(
            "io_write_failed",
            format!("create {}: {e}", dest.display()),
        )
    })?;

    let mut hasher = Sha256::new();
    let mut written: u64 = 0;

    let mut res = res;
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|e| ApiError::new("http_read_failed", format!("read {url}: {e}")))?
    {
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| {
            ApiError::new(
                "io_write_failed",
                format!("write {}: {e}", dest.display()),
            )
        })?;
        written = written.saturating_add(chunk.len() as u64);
    }

    file.flush().await.ok();
    drop(file);

    if expected_size != 0 && written != expected_size {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(ApiError::new(
            "asset_size_mismatch",
            format!("downloaded {written} bytes, expected {expected_size}"),
        ));
    }

    let got = digest_hex_lower(hasher.finalize());
    if got != expected_sha256 {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(ApiError::new(
            "asset_sha256_mismatch",
            format!("downloaded sha256 {got}, expected {expected_sha256}"),
        ));
    }

    Ok(())
}

fn sha256_file_hex(path: &Path) -> ApiResult<String> {
    let mut f = std::fs::File::open(path).map_err(|e| {
        ApiError::new("io_read_failed", format!("open {}: {e}", path.display()))
    })?;

    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];

    loop {
        let n = std::io::Read::read(&mut f, &mut buf)
            .map_err(|e| ApiError::new("io_read_failed", format!("read {}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(digest_hex_lower(hasher.finalize()))
}

fn digest_hex_lower(digest: impl AsRef<[u8]>) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let b = digest.as_ref();
    let mut out = String::with_capacity(b.len() * 2);
    for &v in b {
        out.push(LUT[(v >> 4) as usize] as char);
        out.push(LUT[(v & 0x0f) as usize] as char);
    }
    out
}

fn http_status_error(url: &str, status: StatusCode) -> ApiError {
    ApiError::new("http_status", format!("GET {url}: {status}"))
        .with_details(serde_json::json!({"url": url, "status": status.as_u16()}))
}
