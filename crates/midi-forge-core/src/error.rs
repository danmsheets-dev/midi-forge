use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    #[error("UMP message type 0x{0:X} is reserved or unsupported")]
    UnknownMessageType(u8),
    #[error("UMP packet needs {needed} word(s), got {got}")]
    WrongWordCount { needed: usize, got: usize },
    #[error("UMP packet cannot be empty")]
    EmptyPacket,
}
