// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use ms_manager_core::{
    extract_tags_from_releases_atom, latest_tag_for_channel, latest_tag_for_channel_from_releases,
    parse_manifest_json, parse_releases_api_json, verify_manifest_sig_b64, Channel, Manifest,
    ManifestChannel,
};

const DIST_SLUG: &str = "petitechose-midi-studio/distribution";

const STABLE_PK_B64: &str = "2rHtM99leFGTpjZ8fZHNCdGXlEKmAw6hEyaat1uGO3M=";
const NIGHTLY_PK_B64: &str = "voOksaS+NoUkEy9c8YunbTwPnb1dlXCyEJ9Yy07233A=";

#[derive(Debug, Clone, serde::Serialize)]
pub struct LatestManifestResponse {
    pub channel: String,
    pub available: bool,
    pub tag: Option<String>,
    pub manifest: Option<Manifest>,
    pub message: Option<String>,
}

fn parse_channel(s: &str) -> Result<Channel, String> {
    match s {
        "stable" => Ok(Channel::Stable),
        "beta" => Ok(Channel::Beta),
        "nightly" => Ok(Channel::Nightly),
        _ => Err(format!("invalid channel: {s}")),
    }
}

fn pk_for_channel(channel: Channel) -> &'static str {
    match channel {
        Channel::Nightly => NIGHTLY_PK_B64,
        Channel::Stable | Channel::Beta => STABLE_PK_B64,
    }
}

fn manifest_urls_for_latest(channel: Channel) -> (String, String) {
    // Only stable has a built-in "latest" endpoint on GitHub.
    // For beta/nightly, resolve the tag first.
    match channel {
        Channel::Stable => (
            format!(
                "https://github.com/{DIST_SLUG}/releases/latest/download/manifest.json"
            ),
            format!(
                "https://github.com/{DIST_SLUG}/releases/latest/download/manifest.json.sig"
            ),
        ),
        Channel::Beta | Channel::Nightly => (String::new(), String::new()),
    }
}

fn manifest_urls_for_tag(tag: &str) -> (String, String) {
    (
        format!(
            "https://github.com/{DIST_SLUG}/releases/download/{tag}/manifest.json",
            tag = tag
        ),
        format!(
            "https://github.com/{DIST_SLUG}/releases/download/{tag}/manifest.json.sig",
            tag = tag
        ),
    )
}

async fn fetch_text(client: &reqwest::Client, url: &str) -> Result<(reqwest::StatusCode, String), String> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = res.status();
    let text = res.text().await.map_err(|e| format!("read {url}: {e}"))?;
    Ok((status, text))
}

async fn fetch_bytes(
    client: &reqwest::Client,
    url: &str,
) -> Result<(reqwest::StatusCode, Vec<u8>), String> {
    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = res.status();
    let bytes = res
        .bytes()
        .await
        .map_err(|e| format!("read {url}: {e}"))?
        .to_vec();
    Ok((status, bytes))
}

async fn resolve_latest_tag(client: &reqwest::Client, channel: Channel) -> Result<Option<String>, String> {
    // Stable "latest" is resolved by directly downloading the manifest.
    if channel == Channel::Stable {
        return Ok(None);
    }

    // 1) Try GitHub Releases API.
    let api_url = format!("https://api.github.com/repos/{DIST_SLUG}/releases?per_page=100");
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

    // 2) Fallback: Atom feed (more tolerant for NAT/rate-limit scenarios).
    let atom_url = format!("https://github.com/{DIST_SLUG}/releases.atom");
    let (status, xml) = fetch_text(client, &atom_url).await?;
    if !status.is_success() {
        return Err(format!("GET {atom_url}: {status}"));
    }
    let tags = extract_tags_from_releases_atom(&xml);
    Ok(latest_tag_for_channel(channel, &tags))
}

fn expected_manifest_channel(channel: Channel) -> ManifestChannel {
    match channel {
        Channel::Stable => ManifestChannel::Stable,
        Channel::Beta => ManifestChannel::Beta,
        Channel::Nightly => ManifestChannel::Nightly,
    }
}

#[tauri::command]
async fn resolve_latest_manifest(channel: String) -> Result<LatestManifestResponse, String> {
    let channel_enum = parse_channel(channel.as_str())?;

    let client = reqwest::Client::builder()
        .user_agent("ms-manager")
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    // Stable: use /latest.
    if channel_enum == Channel::Stable {
        let (manifest_url, sig_url) = manifest_urls_for_latest(channel_enum);
        let (m_status, m_bytes) = fetch_bytes(&client, &manifest_url).await?;
        if m_status == reqwest::StatusCode::NOT_FOUND {
            return Ok(LatestManifestResponse {
                channel,
                available: false,
                tag: None,
                manifest: None,
                message: Some("No stable release published yet.".to_string()),
            });
        }
        if !m_status.is_success() {
            return Err(format!("GET {manifest_url}: {m_status}"));
        }

        let (s_status, sig_text) = fetch_text(&client, &sig_url).await?;
        if !s_status.is_success() {
            return Err(format!("GET {sig_url}: {s_status}"));
        }

        verify_manifest_sig_b64(&m_bytes, &sig_text, pk_for_channel(channel_enum))
            .map_err(|e| format!("signature verify failed: {e}"))?;

        let manifest = parse_manifest_json(&m_bytes).map_err(|e| format!("manifest parse failed: {e}"))?;
        if manifest.channel != expected_manifest_channel(channel_enum) {
            return Err(format!(
                "manifest channel mismatch: expected {:?}, got {:?}",
                expected_manifest_channel(channel_enum),
                manifest.channel
            ));
        }

        return Ok(LatestManifestResponse {
            channel,
            available: true,
            tag: Some(manifest.tag.clone()),
            manifest: Some(manifest),
            message: None,
        });
    }

    // Beta/nightly: resolve tag first.
    let tag = resolve_latest_tag(&client, channel_enum)
        .await?
        .ok_or_else(|| format!("no releases found for channel: {channel}"))?;

    let (manifest_url, sig_url) = manifest_urls_for_tag(&tag);
    let (m_status, m_bytes) = fetch_bytes(&client, &manifest_url).await?;
    if !m_status.is_success() {
        return Err(format!("GET {manifest_url}: {m_status}"));
    }
    let (s_status, sig_text) = fetch_text(&client, &sig_url).await?;
    if !s_status.is_success() {
        return Err(format!("GET {sig_url}: {s_status}"));
    }

    verify_manifest_sig_b64(&m_bytes, &sig_text, pk_for_channel(channel_enum))
        .map_err(|e| format!("signature verify failed: {e}"))?;
    let manifest = parse_manifest_json(&m_bytes).map_err(|e| format!("manifest parse failed: {e}"))?;
    if manifest.channel != expected_manifest_channel(channel_enum) {
        return Err(format!(
            "manifest channel mismatch: expected {:?}, got {:?}",
            expected_manifest_channel(channel_enum),
            manifest.channel
        ));
    }
    if manifest.tag != tag {
        return Err(format!("manifest tag mismatch: expected {tag}, got {}", manifest.tag));
    }

    Ok(LatestManifestResponse {
        channel,
        available: true,
        tag: Some(tag),
        manifest: Some(manifest),
        message: None,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![resolve_latest_manifest])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
