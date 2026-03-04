use thiserror::Error;

/// DNF login system error types
#[derive(Error, Debug)]
pub enum DnfError {
    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Decryption error: {0}")]
    Decryption(String),

    #[error("Token generation error: {0}")]
    TokenGeneration(String),

    #[error("Invalid hex string: {0}")]
    InvalidHex(#[from] hex::FromHexError),

    #[error("Invalid base64: {0}")]
    InvalidBase64(#[from] base64::DecodeError),

    #[error("RSA error: {0}")]
    Rsa(#[from] rsa::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid protocol data")]
    InvalidProtocol,

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("User not found")]
    UserNotFound,

    #[error("User already exists")]
    UserExists,

    #[error("Account banned: {0}")]
    AccountBanned(String),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Invalid username")]
    InvalidUsername,

    #[error("Invalid password")]
    InvalidPassword,

    #[error("Invalid QQ number")]
    InvalidQqNumber,
}

/// Result type alias
pub type Result<T> = std::result::Result<T, DnfError>;
