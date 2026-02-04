use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestChannel {
    Stable,
    Beta,
    Nightly,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub schema: u32,
    pub channel: ManifestChannel,
    pub tag: String,
    pub published_at: String,
    pub repos: Vec<ManifestRepo>,
    pub assets: Vec<ManifestAsset>,
    pub install_sets: Vec<ManifestInstallSet>,
    #[serde(default)]
    pub pages: Option<ManifestPages>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestRepo {
    pub id: String,
    pub url: String,
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestAsset {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    pub filename: String,
    pub size: u64,
    pub sha256: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestInstallSet {
    pub id: String,
    #[serde(default)]
    pub os: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    pub assets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestPages {
    #[serde(default)]
    pub demo_url: Option<String>,
}

pub fn parse_manifest_json(bytes: &[u8]) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(bytes)?;
    if m.schema != 2 {
        return Err(CoreError::UnsupportedSchema(m.schema));
    }
    Ok(m)
}

pub fn select_install_set_assets(
    manifest: &Manifest,
    install_set_id: &str,
    os: &str,
    arch: &str,
) -> Result<Vec<ManifestAsset>> {
    let set = manifest.install_sets.iter().find(|s| {
        s.id == install_set_id && s.os.as_deref() == Some(os) && s.arch.as_deref() == Some(arch)
    });
    let set = match set {
        Some(v) => v,
        None => {
            return Err(CoreError::NoMatchingInstallSet {
                id: install_set_id.to_string(),
                os: os.to_string(),
                arch: arch.to_string(),
            })
        }
    };

    let mut out = Vec::with_capacity(set.assets.len());
    for id in &set.assets {
        let a = manifest
            .assets
            .iter()
            .find(|a| a.id == *id)
            .cloned()
            .ok_or_else(|| CoreError::UnknownAssetId(id.clone()))?;
        out.push(a);
    }
    Ok(out)
}

pub fn select_default_assets(
    manifest: &Manifest,
    os: &str,
    arch: &str,
) -> Result<Vec<ManifestAsset>> {
    select_install_set_assets(manifest, "default", os, arch)
}
