//! UMP-canonical MIDI engine for Midi-Forge.
//!
//! This crate has no OS or GUI dependencies. MIDI 1.0 bytestreams and MIDI 2.0
//! UMP both become [`UmpMessage`] before they enter the router.

mod cc;
mod clock_master;
mod decode;
mod devices;
mod error;
mod event;
mod filter;
mod hang;
mod live;
mod log;
mod map;
mod mfr;
mod midi1;
mod midi2;
mod midi_ci;
mod mpe;
mod netump;
mod nrpn;
mod packs;
mod panic;
mod profile;
mod route;
mod router;
mod smf;
mod sysex;
mod timing;
mod ump;
mod utility;

pub use cc::{cc_label, cc_name};
pub use clock_master::ClockMaster;
pub use decode::{Decoded, decode};
pub use devices::{DeviceProfile, apply_device, device_library, pack_for_device};
pub use error::CoreError;
pub use event::{MidiEvent, PortId, Timestamp};
pub use filter::{Filter, MessageKind, message_kind};
pub use hang::{HangNote, HangTracker};
pub use live::{LiveChannel, LiveView};
pub use log::MonitorLog;
pub use map::{DataMap, MapAction, MapEntry, MatchKind, Matcher, ValueMap, VoiceKind};
pub use mfr::{manufacturer_label, manufacturer_name};
pub use midi_ci::{
    CiDiscovery, CiPeCaps, CiProfileList, FORGE_MUID, PeData, discovery_inquiry,
    parse_ci_discovery, parse_ci_note, parse_ci_pe_caps, parse_ci_profiles, parse_pe_data,
    pe_capability_inquiry, pe_get, pe_set, profile_inquiry,
};
pub use midi1::{
    Midi1Parser, format_wire_hex, midi1_data_len, packed_short_from_ump, ump_from_packed_short,
    ump_from_status_data,
};
pub use midi2::{
    downscale_to_midi1, midi2_assignable_controller, midi2_assignable_controller_relative,
    midi2_assignable_per_note, midi2_cc, midi2_note_off, midi2_note_off_attr, midi2_note_on,
    midi2_note_on_attr, midi2_per_note_management, midi2_per_note_pitch_bend, midi2_pitch_bend,
    midi2_registered_controller, midi2_registered_controller_relative, midi2_registered_per_note,
    upscale_to_midi2, value7_to_32, value32_to_7, velocity7_to_16, velocity16_to_7,
};
pub use mpe::{
    MPE_CONFIG_RPN, MpeTracker, MpeVoice, MpeZone, MpeZoneKind, PITCH_BEND_RANGE_RPN,
    bend_semitones, mcm_packets, pitch_bend_range_packets,
};
pub use netump::{
    CMD_INVITATION, DEFAULT_PORT as NETUMP_PORT, decode_command, decode_ump, encode_command,
    encode_ump, invitation, looks_like_command,
};
pub use nrpn::{NrpnTracker, ParamKind, ParamValue, rpn_name};
pub use packs::{DumpPack, dump_packs, pack_dump};
pub use panic::panic_packets;
pub use profile::{PROFILE_VERSION, Profile, ProfileLink, Scene};
pub use route::{RouteEvent, RouteLog};
pub use router::{Link, Router};
pub use smf::{SessionRecorder, events_from_smf0, write_smf0};
pub use sysex::{
    IDENTITY_REQUEST, IdentityReply, SysexAssembler, SysexDump, SysexError, dumps_from_hex,
    dumps_from_syx, dumps_to_syx, hex_diff, parse_identity_reply, roland_checksum_from_sum,
};
pub use timing::{ClockHealth, IntervalHist, MtcState, Transport};
pub use ump::UmpMessage;
pub use utility::{ump_dctpq, ump_delta_clockstamp, ump_jr_clock, ump_jr_timestamp, ump_noop};
