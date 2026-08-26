//! UMP-canonical MIDI engine for Midi-Forge.
//!
//! This crate has no OS or GUI dependencies. MIDI 1.0 bytestreams and MIDI 2.0
//! UMP both become [`UmpMessage`] before they enter the router.

mod decode;
mod error;
mod event;
mod midi1;
mod ump;

pub use decode::{Decoded, decode};
pub use error::CoreError;
pub use event::{MidiEvent, PortId, Timestamp};
pub use midi1::Midi1Parser;
pub use ump::UmpMessage;
