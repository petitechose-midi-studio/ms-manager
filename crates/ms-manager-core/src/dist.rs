use crate::Channel;

pub const DIST_REPO_SLUG: &str = "petitechose-midi-studio/distribution";

pub const STABLE_PUBLIC_KEY_B64: &str = "2rHtM99leFGTpjZ8fZHNCdGXlEKmAw6hEyaat1uGO3M=";
pub const NIGHTLY_PUBLIC_KEY_B64: &str = "voOksaS+NoUkEy9c8YunbTwPnb1dlXCyEJ9Yy07233A=";

pub fn public_key_b64_for_channel(channel: Channel) -> &'static str {
    match channel {
        Channel::Nightly => NIGHTLY_PUBLIC_KEY_B64,
        Channel::Stable | Channel::Beta => STABLE_PUBLIC_KEY_B64,
    }
}

pub fn stable_latest_manifest_url() -> String {
    format!("https://github.com/{DIST_REPO_SLUG}/releases/latest/download/manifest.json")
}

pub fn stable_latest_sig_url() -> String {
    format!("https://github.com/{DIST_REPO_SLUG}/releases/latest/download/manifest.json.sig")
}

pub fn manifest_url_for_tag(tag: &str) -> String {
    format!(
        "https://github.com/{DIST_REPO_SLUG}/releases/download/{tag}/manifest.json",
        tag = tag
    )
}

pub fn manifest_sig_url_for_tag(tag: &str) -> String {
    format!(
        "https://github.com/{DIST_REPO_SLUG}/releases/download/{tag}/manifest.json.sig",
        tag = tag
    )
}

pub fn asset_url_for_tag(tag: &str, filename: &str) -> String {
    format!(
        "https://github.com/{DIST_REPO_SLUG}/releases/download/{tag}/{filename}",
        tag = tag,
        filename = filename
    )
}
