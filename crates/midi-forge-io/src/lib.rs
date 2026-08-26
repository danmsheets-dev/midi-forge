//! Platform MIDI backends. The GUI talks to [`MidiBackend`], never to WinMM
//! or CoreMIDI directly.

mod backend;
mod error;
mod null;

#[cfg(windows)]
mod winmm;

pub use backend::{Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint, default_backend};
pub use error::IoError;
pub use null::NullBackend;

#[cfg(windows)]
pub use winmm::WinMmBackend;
