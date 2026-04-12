use thiserror::Error;

/// Domain errors only — no I/O errors. Adapter crates map their I/O errors
/// into these variants via string conversion.
#[derive(Error, Debug)]
pub enum TalkiwiError {
    // Session
    #[error("Session already recording")]
    AlreadyRecording,
    #[error("No active session")]
    NoActiveSession,
    #[error("Session in invalid state: {0}")]
    InvalidState(String),

    // ASR
    #[error("ASR provider not available: {0}")]
    AsrUnavailable(String),
    #[error("ASR transcription failed: {0}")]
    AsrFailed(String),
    #[error("Audio capture failed: {0}")]
    AudioCaptureFailed(String),

    // Capture
    #[error("Capture permission denied: {0}")]
    PermissionDenied(String),
    #[error("Capture module failed: {module} - {reason}")]
    CaptureFailed { module: String, reason: String },

    // Engine
    #[error("Intent provider not available: {0}")]
    IntentUnavailable(String),
    #[error("Intent processing failed: {0}")]
    IntentFailed(String),
    #[error("Intent provider timeout after {0}ms")]
    IntentTimeout(u64),

    // Provider
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),
    #[error("Provider switch failed: {0}")]
    ProviderSwitchFailed(String),

    // Storage — no #[from], mapped by adapter layer
    #[error("Storage error: {0}")]
    Storage(String),

    // General
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("IO error: {0}")]
    Io(String),
}

pub type Result<T> = std::result::Result<T, TalkiwiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_strings() {
        assert_eq!(
            TalkiwiError::AlreadyRecording.to_string(),
            "Session already recording"
        );
        assert_eq!(
            TalkiwiError::Storage("db locked".to_string()).to_string(),
            "Storage error: db locked"
        );
        assert_eq!(
            TalkiwiError::CaptureFailed {
                module: "screenshot".to_string(),
                reason: "permission denied".to_string()
            }
            .to_string(),
            "Capture module failed: screenshot - permission denied"
        );
        assert_eq!(
            TalkiwiError::IntentTimeout(30000).to_string(),
            "Intent provider timeout after 30000ms"
        );
    }
}
