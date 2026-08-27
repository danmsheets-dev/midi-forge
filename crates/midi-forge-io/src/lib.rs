//! Platform MIDI backends. The GUI talks to [`MidiBackend`], never to WinMM
//! or CoreMIDI directly.

mod backend;
mod error;
mod loopback;
mod net;
mod null;
mod occupy;
mod wms;

#[cfg(windows)]
mod winmm;

#[cfg(target_os = "macos")]
#[path = "coremidi.rs"]
mod coremidi_backend;

pub use backend::{
    BackendCaps, Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint, default_backend,
    packets_for_wire,
};
pub use error::IoError;
pub use loopback::is_loopback_pair;
pub use net::NetUmp;
pub use null::NullBackend;
pub use occupy::{explain_in_use, likely_midi_holders};
pub use wms::{WmsStatus, create_wms_loopback, midi_cli_path, probe_wms};

#[cfg(windows)]
pub use winmm::{WinMmBackend, midisrv_running};

#[cfg(target_os = "macos")]
pub use coremidi_backend::CoreMidiBackend;
