use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
    Nightly,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
            Channel::Nightly => "nightly",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct BetaVersion {
    pub base: SemVer,
    pub n: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NightlyDate {
    pub year: u32,
    pub month: u8,
    pub day: u8,
}

pub fn parse_stable_tag(tag: &str) -> Option<SemVer> {
    // vMAJOR.MINOR.PATCH
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^v(\d+)\.(\d+)\.(\d+)$").expect("valid regex"));
    let cap = re.captures(tag)?;
    Some(SemVer {
        major: cap.get(1)?.as_str().parse().ok()?,
        minor: cap.get(2)?.as_str().parse().ok()?,
        patch: cap.get(3)?.as_str().parse().ok()?,
    })
}

pub fn parse_beta_tag(tag: &str) -> Option<BetaVersion> {
    // vMAJOR.MINOR.PATCH-beta.N
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"^v(\d+)\.(\d+)\.(\d+)-beta\.(\d+)$").expect("valid regex"));
    let cap = re.captures(tag)?;
    Some(BetaVersion {
        base: SemVer {
            major: cap.get(1)?.as_str().parse().ok()?,
            minor: cap.get(2)?.as_str().parse().ok()?,
            patch: cap.get(3)?.as_str().parse().ok()?,
        },
        n: cap.get(4)?.as_str().parse().ok()?,
    })
}

pub fn parse_nightly_tag(tag: &str) -> Option<NightlyDate> {
    // nightly-YYYY-MM-DD
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"^nightly-(\d{4})-(\d{2})-(\d{2})$").expect("valid regex"));
    let cap = re.captures(tag)?;
    Some(NightlyDate {
        year: cap.get(1)?.as_str().parse().ok()?,
        month: cap.get(2)?.as_str().parse().ok()?,
        day: cap.get(3)?.as_str().parse().ok()?,
    })
}

pub fn is_tag_for_channel(channel: Channel, tag: &str) -> bool {
    match channel {
        Channel::Stable => parse_stable_tag(tag).is_some(),
        Channel::Beta => parse_beta_tag(tag).is_some(),
        Channel::Nightly => parse_nightly_tag(tag).is_some(),
    }
}

pub fn compare_tags(channel: Channel, a: &str, b: &str) -> Option<Ordering> {
    match channel {
        Channel::Stable => Some(parse_stable_tag(a)?.cmp(&parse_stable_tag(b)?)),
        Channel::Beta => Some(parse_beta_tag(a)?.cmp(&parse_beta_tag(b)?)),
        Channel::Nightly => Some(parse_nightly_tag(a)?.cmp(&parse_nightly_tag(b)?)),
    }
}
