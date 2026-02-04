//! Core distribution logic for `ms-manager`.
//!
//! This crate is intentionally UI-agnostic and does not depend on Tauri.

mod channel;
mod crypto;
mod dist;
mod error;
mod github;
mod install_state;
mod manifest;
mod platform;
mod settings;

pub use channel::{compare_tags, BetaVersion, Channel, NightlyDate, SemVer};
pub use crypto::{decode_b64_32, sha256_hex, verify_manifest_sig_b64};
pub use dist::{
    asset_url_for_tag, manifest_sig_url_for_tag, manifest_url_for_tag, public_key_b64_for_channel,
    stable_latest_manifest_url, stable_latest_sig_url, DIST_REPO_SLUG, NIGHTLY_PUBLIC_KEY_B64,
    STABLE_PUBLIC_KEY_B64,
};
pub use error::{CoreError, Result};
pub use github::{
    extract_tags_from_releases_atom, latest_tag_for_channel, latest_tag_for_channel_from_releases,
    parse_releases_api_json, ReleaseInfo,
};
pub use install_state::{InstallState, INSTALL_STATE_SCHEMA};
pub use manifest::{
    parse_manifest_json, select_default_assets, select_install_set_assets, Manifest, ManifestAsset,
    ManifestChannel,
};
pub use platform::{Arch, Os, Platform};
pub use settings::{Settings, SETTINGS_SCHEMA};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn latest_tag_for_beta_picks_highest_semver_then_beta_n() {
        let tags = [
            "v0.0.1-beta.9",
            "v0.0.2-beta.1",
            "v0.0.1-beta.10",
            "v0.0.2-beta.2",
        ]
        .into_iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>();

        let got = latest_tag_for_channel(Channel::Beta, &tags).unwrap();
        assert_eq!(got, "v0.0.2-beta.2");
    }

    #[test]
    fn latest_tag_for_nightly_picks_latest_date() {
        let tags = [
            "nightly-2026-02-01",
            "nightly-2026-02-02",
            "nightly-2025-12-31",
        ]
        .into_iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>();

        let got = latest_tag_for_channel(Channel::Nightly, &tags).unwrap();
        assert_eq!(got, "nightly-2026-02-02");
    }

    #[test]
    fn atom_parser_extracts_tags() {
        let xml = r#"<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<feed xmlns=\"http://www.w3.org/2005/Atom\">
  <entry>
    <link rel=\"alternate\" type=\"text/html\" href=\"https://github.com/petitechose-midi-studio/distribution/releases/tag/v0.0.1-beta.2\"/>
  </entry>
  <entry>
    <link rel=\"alternate\" type=\"text/html\" href=\"https://github.com/petitechose-midi-studio/distribution/releases/tag/nightly-2026-02-02\"/>
  </entry>
</feed>
"#;

        let tags = extract_tags_from_releases_atom(xml);
        assert_eq!(tags, vec!["v0.0.1-beta.2", "nightly-2026-02-02"]);
    }

    #[test]
    fn select_default_assets_picks_matching_os_arch() {
        let json = r#"{
  "schema": 2,
  "channel": "nightly",
  "tag": "nightly-2026-02-02",
  "published_at": "2026-02-02T05:14:21Z",
  "repos": [{"id": "loader", "url": "https://example.invalid", "sha": "0000000000000000000000000000000000000000"}],
  "assets": [
    {
      "id": "bundle-linux-x86_64",
      "kind": "bundle",
      "os": "linux",
      "arch": "x86_64",
      "filename": "midi-studio-linux-x86_64-bundle.zip",
      "size": 1,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
    }
  ],
  "install_sets": [
    {"id": "default", "os": "linux", "arch": "x86_64", "assets": ["bundle-linux-x86_64"]}
  ]
}"#;

        let m = parse_manifest_json(json.as_bytes()).unwrap();
        let assets = select_default_assets(&m, "linux", "x86_64").unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].filename, "midi-studio-linux-x86_64-bundle.zip");
    }

    #[test]
    fn compare_tags_stable_orders_semver() {
        assert_eq!(
            compare_tags(Channel::Stable, "v0.1.0", "v0.2.0"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_tags_beta_orders_beta_n() {
        assert_eq!(
            compare_tags(Channel::Beta, "v0.1.0-beta.2", "v0.1.0-beta.10"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_tags_nightly_orders_date() {
        assert_eq!(
            compare_tags(Channel::Nightly, "nightly-2026-02-01", "nightly-2026-02-02"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn select_install_set_assets_picks_profile_id() {
        let json = r#"{
  "schema": 2,
  "channel": "beta",
  "tag": "v0.1.0-beta.1",
  "published_at": "2026-02-02T05:14:21Z",
  "repos": [{"id": "loader", "url": "https://example.invalid", "sha": "0000000000000000000000000000000000000000"}],
  "assets": [
    {
      "id": "bundle-linux-x86_64",
      "kind": "bundle",
      "os": "linux",
      "arch": "x86_64",
      "filename": "midi-studio-linux-x86_64-bundle.zip",
      "size": 1,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    {
      "id": "firmware-default",
      "kind": "firmware",
      "filename": "midi-studio-default-firmware.hex",
      "size": 1,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    {
      "id": "firmware-bitwig",
      "kind": "firmware",
      "filename": "midi-studio-bitwig-firmware.hex",
      "size": 1,
      "sha256": "0000000000000000000000000000000000000000000000000000000000000000"
    }
  ],
  "install_sets": [
    {"id": "default", "os": "linux", "arch": "x86_64", "assets": ["bundle-linux-x86_64", "firmware-default"]},
    {"id": "bitwig", "os": "linux", "arch": "x86_64", "assets": ["bundle-linux-x86_64", "firmware-bitwig"]}
  ]
}"#;

        let m = parse_manifest_json(json.as_bytes()).unwrap();
        let assets = select_install_set_assets(&m, "bitwig", "linux", "x86_64").unwrap();
        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].id, "bundle-linux-x86_64");
        assert_eq!(assets[1].id, "firmware-bitwig");
    }
}
