//! UMP-canonical MIDI engine for Midi-Forge.
//!
//! This crate has no OS or GUI dependencies. MIDI 1.0 bytestreams and MIDI 2.0
//! UMP both become [`UmpMessage`] before they enter the router.

mod decode;
mod error;
mod event;
mod filter;
mod hang;
mod log;
mod map;
mod midi1;
mod midi2;
mod mpe;
mod panic;
mod profile;
mod router;
mod sysex;
mod ump;

pub use decode::{Decoded, decode};
pub use error::CoreError;
pub use event::{MidiEvent, PortId, Timestamp};
pub use filter::{Filter, MessageKind, message_kind};
pub use hang::{HangNote, HangTracker};
pub use log::MonitorLog;
pub use map::{DataMap, MapAction, MapEntry, MatchKind, Matcher, ValueMap, VoiceKind};
pub use midi1::{
    Midi1Parser, format_wire_hex, midi1_data_len, packed_short_from_ump, ump_from_packed_short,
    ump_from_status_data,
};
pub use midi2::{downscale_to_midi1, value32_to_7, velocity16_to_7};
pub use mpe::{
    MPE_CONFIG_RPN, MpeTracker, MpeVoice, MpeZone, MpeZoneKind, PITCH_BEND_RANGE_RPN,
    bend_semitones, mcm_packets,
};
pub use panic::panic_packets;
pub use profile::{PROFILE_VERSION, Profile, ProfileLink};
pub use router::{Link, Router};
pub use sysex::{
    IDENTITY_REQUEST, IdentityReply, SysexAssembler, SysexDump, SysexError, dumps_from_hex,
    dumps_from_syx, dumps_to_syx, parse_identity_reply, roland_checksum_from_sum,
};
pub use ump::UmpMessage;
