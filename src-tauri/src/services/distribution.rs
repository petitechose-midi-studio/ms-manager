use ms_manager_core::{
    compare_tags, extract_tags_from_releases_atom, is_tag_for_channel, latest_tag_for_channel,
    latest_tag_for_channel_from_releases, manifest_sig_url_for_tag, manifest_url_for_tag,
    parse_manifest_json, parse_releases_api_json, public_key_b64_for_channel,
    stable_latest_manifest_url, stable_latest_sig_url, verify_manifest_sig_b64, Channel, Manifest,
    ManifestChannel, DIST_REPO_SLUG,
};

use crate::api_error::{ApiError, ApiResult};

pub struct LatestManifest {
    pub available: bool,
    pub tag: Option<String>,
    pub manifest: Option<Manifest>,
    pub message: Option<String>,
}

pub async fn resolve_latest_manifest(
    client: &reqwest::Client,
    channel: Channel,
) -> ApiResult<LatestManifest> {
    // Stable: use /latest.
    if channel == Channel::Stable {
        let manifest_url = stable_latest_manifest_url();
        let sig_url = stable_latest_sig_url();

        let (m_status, m_bytes) = fetch_bytes(client, &manifest_url).await?;
        if m_status == reqwest::StatusCode::NOT_FOUND {
            return Ok(LatestManifest {
                available: false,
                tag: None,
                manifest: None,
                message: Some("No stable release published yet.".to_string()),
            });
        }
        if !m_status.is_success() {
            return Err(http_status_error(&manifest_url, m_status));
        }
        let (s_status, sig_text) = fetch_text(client, &sig_url).await?;
        if !s_status.is_success() {
            return Err(http_status_error(&sig_url, s_status));
        }

        verify_manifest_sig_b64(&m_bytes, &sig_text, public_key_b64_for_channel(channel))?;
        let manifest = parse_manifest_json(&m_bytes)?;
        ensure_manifest_channel(channel, &manifest)?;

        return Ok(LatestManifest {
            available: true,
            tag: Some(manifest.tag.clone()),
            manifest: Some(manifest),
            message: None,
        });
    }

    // Beta/nightly: resolve tag first.
    let tag = resolve_latest_tag(client, channel)
        .await?
        .ok_or_else(|| ApiError::new("no_releases", "no releases found for channel"))?;

    let manifest_url = manifest_url_for_tag(&tag);
    let sig_url = manifest_sig_url_for_tag(&tag);

    let (m_status, m_bytes) = fetch_bytes(client, &manifest_url).await?;
    if !m_status.is_success() {
        return Err(http_status_error(&manifest_url, m_status));
    }

    let (s_status, sig_text) = fetch_text(client, &sig_url).await?;
    if !s_status.is_success() {
        return Err(http_status_error(&sig_url, s_status));
    }

    verify_manifest_sig_b64(&m_bytes, &sig_text, public_key_b64_for_channel(channel))?;
    let manifest = parse_manifest_json(&m_bytes)?;
    ensure_manifest_channel(channel, &manifest)?;
    if manifest.tag != tag {
        return Err(ApiError::new(
            "manifest_tag_mismatch",
            format!("expected tag {tag}, got {}", manifest.tag),
        ));
    }

    Ok(LatestManifest {
        available: true,
        tag: Some(tag),
        manifest: Some(manifest),
        message: None,
    })
}

pub async fn resolve_manifest_for_tag(
    client: &reqwest::Client,
    channel: Channel,
    tag: &str,
) -> ApiResult<LatestManifest> {
    let manifest_url = manifest_url_for_tag(tag);
    let sig_url = manifest_sig_url_for_tag(tag);

    let (m_status, m_bytes) = fetch_bytes(client, &manifest_url).await?;
    if !m_status.is_success() {
        if m_status == reqwest::StatusCode::NOT_FOUND {
            return Ok(LatestManifest {
                available: false,
                tag: None,
                manifest: None,
                message: Some("Release not found.".to_string()),
            });
        }
        return Err(http_status_error(&manifest_url, m_status));
    }

    let (s_status, sig_text) = fetch_text(client, &sig_url).await?;
    if !s_status.is_success() {
        return Err(http_status_error(&sig_url, s_status));
    }

    verify_manifest_sig_b64(&m_bytes, &sig_text, public_key_b64_for_channel(channel))?;
    let manifest = parse_manifest_json(&m_bytes)?;
    ensure_manifest_channel(channel, &manifest)?;
    if manifest.tag != tag {
        return Err(ApiError::new(
            "manifest_tag_mismatch",
            format!("expected tag {tag}, got {}", manifest.tag),
        ));
    }

    Ok(LatestManifest {
        available: true,
        tag: Some(tag.to_string()),
        manifest: Some(manifest),
        message: None,
    })
}

pub async fn list_tags_for_channel(
    client: &reqwest::Client,
    channel: Channel,
) -> ApiResult<Vec<String>> {
    // 1) Try GitHub Releases API.
    let api_url = format!("https://api.github.com/repos/{DIST_REPO_SLUG}/releases?per_page=100");
    if let Ok((status, body)) = fetch_text(client, &api_url).await {
        if status.is_success() {
            if let Ok(releases) = parse_releases_api_json(&body) {
                let mut tags = releases
                    .into_iter()
                    .filter(|r| !r.draft)
                    .filter(|r| match channel {
                        Channel::Stable => !r.prerelease,
                        Channel::Beta | Channel::Nightly => r.prerelease,
                    })
                    .map(|r| r.tag)
                    .filter(|t| is_tag_for_channel(channel, t))
                    .collect::<Vec<_>>();
                sort_tags(channel, &mut tags);
                tags.dedup();
                if !tags.is_empty() {
                    return Ok(tags);
                }
            }
        }
    }

    // 2) Fallback: Atom feed.
    let atom_url = format!("https://github.com/{DIST_REPO_SLUG}/releases.atom");
    let (status, xml) = fetch_text(client, &atom_url).await?;
    if !status.is_success() {
        return Err(http_status_error(&atom_url, status));
    }
    let mut tags = extract_tags_from_releases_atom(&xml)
        .into_iter()
        .filter(|t| is_tag_for_channel(channel, t))
        .collect::<Vec<_>>();
    sort_tags(channel, &mut tags);
    tags.dedup();
    Ok(tags)
}

fn sort_tags(channel: Channel, tags: &mut [String]) {
    tags.sort_by(|a, b| {
        let ord = compare_tags(channel, a, b).unwrap_or(std::cmp::Ordering::Equal);
        ord.reverse()
    });
}

async fn resolve_latest_tag(
    client: &reqwest::Client,
    channel: Channel,
) -> ApiResult<Option<String>> {
    // 1) Try GitHub Releases API.
    let api_url = format!("https://api.github.com/repos/{DIST_REPO_SLUG}/releases?per_page=100");
    if let Ok((status, body)) = fetch_text(client, &api_url).await {
        if status.is_success() {
            if let Ok(releases) = parse_releases_api_json(&body) {
                if let Ok(tag) = latest_tag_for_channel_from_releases(channel, &releases) {
                    if tag.is_some() {
                        return Ok(tag);
                    }
                }
            }
        }
    }

    // 2) Fallback: Atom feed.
    let atom_url = format!("https://github.com/{DIST_REPO_SLUG}/releases.atom");
    let (status, xml) = fetch_text(client, &atom_url).await?;
    if !status.is_success() {
        return Err(http_status_error(&atom_url, status));
    }
    let tags = extract_tags_from_releases_atom(&xml);
    Ok(latest_tag_for_channel(channel, &tags))
}

fn ensure_manifest_channel(channel: Channel, manifest: &Manifest) -> ApiResult<()> {
    let expected = match channel {
        Channel::Stable => ManifestChannel::Stable,
        Channel::Beta => ManifestChannel::Beta,
        Channel::Nightly => ManifestChannel::Nightly,
    };
    if manifest.channel != expected {
        return Err(ApiError::new(
            "manifest_channel_mismatch",
            format!("expected {expected:?}, got {:?}", manifest.channel),
        ));
    }
    Ok(())
}

fn http_status_error(url: &str, status: reqwest::StatusCode) -> ApiError {
    ApiError::new("http_status", format!("GET {url}: {status}"))
        .with_details(serde_json::json!({"url": url, "status": status.as_u16()}))
}

async fn fetch_text(
    client: &reqwest::Client,
    url: &str,
) -> ApiResult<(reqwest::StatusCode, String)> {
    let res = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            ApiError::new("http_request_failed", format!("GET {url}: {e}"))
                .with_details(serde_json::json!({"url": url}))
        })?;
    let status = res.status();
    let text = res.text().await.map_err(|e| {
        ApiError::new("http_read_failed", format!("read {url}: {e}"))
            .with_details(serde_json::json!({"url": url}))
    })?;
    Ok((status, text))
}

async fn fetch_bytes(
    client: &reqwest::Client,
    url: &str,
) -> ApiResult<(reqwest::StatusCode, Vec<u8>)> {
    let res = client.get(url).send().await.map_err(|e| {
        ApiError::new("http_request_failed", format!("GET {url}: {e}"))
            .with_details(serde_json::json!({"url": url}))
    })?;
    let status = res.status();
    let bytes = res.bytes().await.map_err(|e| {
        ApiError::new("http_read_failed", format!("read {url}: {e}"))
            .with_details(serde_json::json!({"url": url}))
    })?;
    Ok((status, bytes.to_vec()))
}
