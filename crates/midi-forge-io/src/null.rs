use crate::backend::{Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint};
use crate::error::IoError;

/// In-memory backend for tests. Never touches the OS.
pub struct NullBackend {
    endpoints: Vec<Endpoint>,
}

impl NullBackend {
    pub fn empty() -> Self {
        Self {
            endpoints: Vec::new(),
        }
    }

    pub fn with_fixture_ports() -> Self {
        Self {
            endpoints: vec![
                Endpoint {
                    id: EndpointId("null:in:0".into()),
                    name: "Null Keyboard".into(),
                    direction: Direction::Input,
                    protocol: ProtocolHint::Midi1Bytes,
                },
                Endpoint {
                    id: EndpointId("null:out:0".into()),
                    name: "Null Synth".into(),
                    direction: Direction::Output,
                    protocol: ProtocolHint::Midi1Bytes,
                },
            ],
        }
    }
}

impl MidiBackend for NullBackend {
    fn name(&self) -> &'static str {
        "null"
    }

    fn refresh(&mut self) -> Result<(), IoError> {
        Ok(())
    }

    fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_lists_keyboard_and_synth() {
        let mut backend = NullBackend::with_fixture_ports();
        backend.refresh().unwrap();
        let names: Vec<_> = backend
            .endpoints()
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, ["Null Keyboard", "Null Synth"]);
        assert_eq!(backend.endpoints()[0].id.0, "null:in:0");
        assert_eq!(backend.endpoints()[0].direction, Direction::Input);
    }
}
