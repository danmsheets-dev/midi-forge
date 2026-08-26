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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub id: EndpointId,
    pub name: String,
    pub direction: Direction,
    pub protocol: ProtocolHint,
}

/// Platform MIDI I/O. Phase 0 only requires enumeration.
pub trait MidiBackend {
    fn name(&self) -> &'static str;
    fn refresh(&mut self) -> Result<(), IoError>;
    fn endpoints(&self) -> &[Endpoint];
}

pub fn default_backend() -> Box<dyn MidiBackend> {
    #[cfg(windows)]
    {
        Box::new(crate::winmm::WinMmBackend::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(crate::null::NullBackend::empty())
    }
}
