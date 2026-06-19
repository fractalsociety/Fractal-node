//! Fractal Society error types

/// Fractal Society error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid artifact format
    #[error("Invalid artifact format: {0}")]
    InvalidArtifact(String),

    /// Artifact not found
    #[error("Artifact not found: {0}")]
    ArtifactNotFound(String),

    /// Verification failed
    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Protocol violation
    #[error("Protocol violation: {0}")]
    ProtocolViolation(String),

    /// Invalid action
    #[error("Invalid action: {0}")]
    InvalidAction(String),

    /// Sandbox violation
    #[error("Sandbox violation: {0}")]
    SandboxViolation(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Data gap detected
    #[error("Data gap detected: {0}")]
    DataGap(String),

    /// Signature creation or verification failure
    #[error("Signature error: {0}")]
    Signature(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Fractal Society result type
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::InvalidArtifact("test".to_string());
        assert_eq!(err.to_string(), "Invalid artifact format: test");
    }
}
