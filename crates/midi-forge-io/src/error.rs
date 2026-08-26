use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("MIDI backend error: {0}")]
    Backend(String),
    #[error("endpoint not found: {0}")]
    NotFound(String),
    #[error("{0} is already open")]
    AlreadyOpen(String),
    #[error("{0} is in use by another application")]
    InUse(String),
    #[error("cannot send this packet to a MIDI 1.0 short-message port")]
    UnsupportedPacket,
}
