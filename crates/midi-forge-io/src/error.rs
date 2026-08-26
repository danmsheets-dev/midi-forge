use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("MIDI backend error: {0}")]
    Backend(String),
    #[error("endpoint not found: {0}")]
    NotFound(String),
    #[error("opening streams is not implemented in Phase 0")]
    NotImplemented,
}
