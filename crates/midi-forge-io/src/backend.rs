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

impl ProtocolHint {
    pub fn label(self) -> &'static str {
        match self {
            Self::Midi1Bytes => "MIDI 1",
            Self::Ump => "UMP",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub id: EndpointId,
    pub name: String,
    pub direction: Direction,
    pub protocol: ProtocolHint,
}

/// Platform MIDI I/O.
pub trait MidiBackend {
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
    use super::ProtocolHint;

    #[test]
    fn protocol_labels() {
        assert_eq!(ProtocolHint::Midi1Bytes.label(), "MIDI 1");
        assert_eq!(ProtocolHint::Ump.label(), "UMP");
    }
}
