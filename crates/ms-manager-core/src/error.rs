use thiserror::Error;

pub type Result<T, E = CoreError> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid base64 value")]
    Base64(#[from] base64::DecodeError),

    #[error("invalid Ed25519 signature")]
    Signature,

    #[error("invalid public key")]
    PublicKey,

    #[error("invalid manifest JSON")]
    ManifestJson(#[from] serde_json::Error),

    #[error("unsupported manifest schema: {0}")]
    UnsupportedSchema(u32),

    #[error("unexpected channel value: {0}")]
    InvalidChannel(String),

    #[error("no matching install set for os={os} arch={arch}")]
    NoMatchingInstallSet { os: String, arch: String },

    #[error("install set references unknown asset id: {0}")]
    UnknownAssetId(String),

    #[error("unsupported platform: os={os} arch={arch}")]
    UnsupportedPlatform { os: String, arch: String },
}
