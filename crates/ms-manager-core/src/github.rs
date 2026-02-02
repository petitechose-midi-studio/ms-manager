use regex::Regex;
use serde::Deserialize;

use crate::channel::{parse_beta_tag, parse_nightly_tag, parse_stable_tag, Channel};
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag: String,
    pub prerelease: bool,
    pub draft: bool,
}

#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    prerelease: bool,
    draft: bool,
}

pub fn parse_releases_api_json(json: &str) -> Result<Vec<ReleaseInfo>> {
    let raw: Vec<ApiRelease> = serde_json::from_str(json)?;
    Ok(raw
        .into_iter()
        .map(|r| ReleaseInfo {
            tag: r.tag_name,
            prerelease: r.prerelease,
            draft: r.draft,
        })
        .collect())
}

pub fn extract_tags_from_releases_atom(xml: &str) -> Vec<String> {
    // Example in the Atom feed:
    // <link ... href="https://github.com/<org>/<repo>/releases/tag/<tag>"/>
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"https://github\.com/[^/]+/[^/]+/releases/tag/([A-Za-z0-9._-]+)")
            .expect("valid regex")
    });

    let mut out = Vec::<String>::new();
    let mut seen = std::collections::HashSet::<String>::new();
    for cap in re.captures_iter(xml) {
        if let Some(m) = cap.get(1) {
            let t = m.as_str().to_string();
            if seen.insert(t.clone()) {
                out.push(t);
            }
        }
    }
    out
}

pub fn latest_tag_for_channel(channel: Channel, tags: &[String]) -> Option<String> {
    match channel {
        Channel::Stable => tags
            .iter()
            .filter_map(|t| parse_stable_tag(t).map(|v| (v, t)))
            .max_by_key(|(v, _)| *v)
            .map(|(_, t)| t.to_string()),

        Channel::Beta => tags
            .iter()
            .filter_map(|t| parse_beta_tag(t).map(|v| (v, t)))
            .max_by_key(|(v, _)| *v)
            .map(|(_, t)| t.to_string()),

        Channel::Nightly => tags
            .iter()
            .filter_map(|t| parse_nightly_tag(t).map(|d| (d, t)))
            .max_by_key(|(d, _)| *d)
            .map(|(_, t)| t.to_string()),
    }
}

pub fn latest_tag_for_channel_from_releases(
    channel: Channel,
    releases: &[ReleaseInfo],
) -> Result<Option<String>> {
    let mut tags: Vec<String> = Vec::new();
    for r in releases {
        if r.draft {
            continue;
        }

        match channel {
            Channel::Stable => {
                if r.prerelease {
                    continue;
                }
            }
            Channel::Beta | Channel::Nightly => {
                if !r.prerelease {
                    continue;
                }
            }
        }

        tags.push(r.tag.clone());
    }

    Ok(latest_tag_for_channel(channel, &tags))
}
