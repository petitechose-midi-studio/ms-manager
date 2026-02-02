use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<ms_manager_core::CoreError> for ApiError {
    fn from(err: ms_manager_core::CoreError) -> Self {
        use ms_manager_core::CoreError;

        match err {
            CoreError::Signature => ApiError::new("manifest_sig_invalid", err.to_string()),
            CoreError::PublicKey => ApiError::new("public_key_invalid", err.to_string()),
            CoreError::UnsupportedSchema(_) => ApiError::new("manifest_schema_unsupported", err.to_string()),
            CoreError::ManifestJson(_) => ApiError::new("manifest_json_invalid", err.to_string()),
            CoreError::NoMatchingInstallSet { .. } => ApiError::new("no_matching_install_set", err.to_string()),
            CoreError::UnknownAssetId(_) => ApiError::new("manifest_invalid_install_set", err.to_string()),
            CoreError::UnsupportedPlatform { .. } => ApiError::new("unsupported_platform", err.to_string()),
            CoreError::Base64(_) => ApiError::new("base64_invalid", err.to_string()),
            CoreError::InvalidChannel(_) => ApiError::new("invalid_channel", err.to_string()),
        }
    }
}
