use midi_forge_core::{MidiEvent, PortId, UmpMessage};

use crate::error::IoError;

/// Stable id for an OS endpoint. Format is backend-specific, e.g. `winmm:in:0`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct EndpointId(pub String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
    Bidirectional,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolHint {
    Midi1Bytes,
    Ump,
}

/// What this backend can do natively. Phase 1: WinMM is MIDI 1 wire;
/// loopbacks preserve UMP. Native `MidiSession` is a later backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendCaps {
    pub native_ump: bool,
    pub scheduled_send: bool,
    pub daw_visible_virtual: bool,
    pub multi_client: bool,
}

impl Default for BackendCaps {
    fn default() -> Self {
        Self {
            native_ump: false,
            scheduled_send: false,
            daw_visible_virtual: false,
            multi_client: false,
        }
    }
}

impl ProtocolHint {
    pub fn label(self) -> &'static str {
        match self {
            Self::Midi1Bytes => "MIDI 1",
            Self::Ump => "UMP",
        }
    }
}

/// Packets to put on the wire for this endpoint. Downscale is per-endpoint,
/// not backend-wide: UMP dests keep type `0x4`; MIDI 1 dests project to `0x2`.
pub fn packets_for_wire(protocol: ProtocolHint, packet: &UmpMessage) -> Vec<UmpMessage> {
    match protocol {
        ProtocolHint::Ump => vec![*packet],
        ProtocolHint::Midi1Bytes => midi_forge_core::downscale_to_midi1(packet),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub id: EndpointId,
    pub name: String,
    pub direction: Direction,
    pub protocol: ProtocolHint,
}

/// Platform MIDI I/O. `Send` so the engine thread can own the backend
/// (WinMM access is still mutex-serialized).
pub trait MidiBackend: Send {
    fn name(&self) -> &'static str;
    fn refresh(&mut self) -> Result<(), IoError>;
    fn endpoints(&self) -> &[Endpoint];
    fn open_input(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError>;
    fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError>;
    fn open_output(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError>;
    fn close_output(&mut self, id: &EndpointId) -> Result<(), IoError>;
    /// Drain captured events. Returns the cumulative count of frames dropped
    /// because the capture queue was full.
    fn poll(&mut self, out: &mut Vec<MidiEvent>) -> u64;
    fn send(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError>;
    /// Send a complete SysEx dump (`F0…F7`) as a long message.
    fn send_sysex(&mut self, id: &EndpointId, bytes: &[u8]) -> Result<(), IoError>;
    /// App-local virtual cable pair (in, out). Other processes do not see these.
    fn create_loopback(&mut self, name: &str) -> Result<(EndpointId, EndpointId), IoError>;
    fn remove_loopback(&mut self, id: &EndpointId) -> Result<(), IoError>;
    fn caps(&self) -> BackendCaps {
        BackendCaps::default()
    }
}

pub fn default_backend() -> Box<dyn MidiBackend> {
    #[cfg(windows)]
    {
        Box::new(crate::winmm::WinMmBackend::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(crate::coremidi_backend::CoreMidiBackend::new())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Box::new(crate::null::NullBackend::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProtocolHint, packets_for_wire};

    #[test]
    fn protocol_labels() {
        assert_eq!(ProtocolHint::Midi1Bytes.label(), "MIDI 1");
        assert_eq!(ProtocolHint::Ump.label(), "UMP");
    }

    #[test]
    fn default_caps_are_midi1_wire() {
        let c = super::BackendCaps::default();
        assert!(!c.native_ump);
        assert!(!c.scheduled_send);
    }

    #[test]
    fn midi2_note_on_ump_dest_stays_type_4() {
        let m2 = midi_forge_core::midi2_note_on(0, 1, 64, 0x8000);
        let out = packets_for_wire(ProtocolHint::Ump, &m2);
        assert_eq!(out, vec![m2]);
        assert_eq!(out[0].message_type(), 0x4);
    }

    #[test]
    fn midi2_note_on_midi1_dest_is_type_2() {
        let m2 = midi_forge_core::midi2_note_on(0, 1, 64, 0x8000);
        let out = packets_for_wire(ProtocolHint::Midi1Bytes, &m2);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].message_type(), 0x2);
        assert_ne!(out[0], m2);
    }
}
