//! UMP-canonical MIDI engine for Midi-Forge.
//!
//! This crate has no OS or GUI dependencies. MIDI 1.0 bytestreams and MIDI 2.0
//! UMP both become [`UmpMessage`] before they enter the router.

mod decode;
mod error;
mod event;
mod log;
mod midi1;
mod panic;
mod ump;

pub use decode::{Decoded, decode};
pub use error::CoreError;
pub use event::{MidiEvent, PortId, Timestamp};
pub use log::MonitorLog;
pub use midi1::{
    Midi1Parser, format_wire_hex, midi1_data_len, packed_short_from_ump, ump_from_packed_short,
    ump_from_status_data,
};
pub use panic::panic_packets;
pub use ump::UmpMessage;
