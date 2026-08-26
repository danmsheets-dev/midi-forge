use std::collections::HashSet;

use midi_forge_core::{MidiEvent, PortId, UmpMessage};

use crate::backend::{Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint};
use crate::error::IoError;

/// In-memory backend for tests. Never touches the OS.
pub struct NullBackend {
    endpoints: Vec<Endpoint>,
    open_inputs: HashSet<String>,
    open_outputs: HashSet<String>,
    pending: Vec<MidiEvent>,
    sent: Vec<(String, UmpMessage)>,
    dropped: u64,
}

impl NullBackend {
    pub fn empty() -> Self {
        Self {
            endpoints: Vec::new(),
            open_inputs: HashSet::new(),
            open_outputs: HashSet::new(),
            pending: Vec::new(),
            sent: Vec::new(),
            dropped: 0,
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
            open_inputs: HashSet::new(),
            open_outputs: HashSet::new(),
            pending: Vec::new(),
            sent: Vec::new(),
            dropped: 0,
        }
    }

    pub fn inject(&mut self, event: MidiEvent) {
        self.pending.push(event);
    }

    pub fn sent(&self) -> &[(String, UmpMessage)] {
        &self.sent
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

    fn open_input(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        self.require(id, Direction::Input)?;
        if !self.open_inputs.insert(id.0.clone()) {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        Ok(())
    }

    fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError> {
        self.open_inputs.remove(&id.0);
        Ok(())
    }

    fn open_output(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        self.require(id, Direction::Output)?;
        if !self.open_outputs.insert(id.0.clone()) {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        Ok(())
    }

    fn close_output(&mut self, id: &EndpointId) -> Result<(), IoError> {
        self.open_outputs.remove(&id.0);
        Ok(())
    }

    fn poll(&mut self, out: &mut Vec<MidiEvent>) -> u64 {
        out.append(&mut self.pending);
        self.dropped
    }

    fn send(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError> {
        if !self.open_outputs.contains(&id.0) {
            return Err(IoError::NotFound(id.0.clone()));
        }
        self.sent.push((id.0.clone(), *packet));
        Ok(())
    }
}

impl NullBackend {
    fn require(&self, id: &EndpointId, direction: Direction) -> Result<(), IoError> {
        if self
            .endpoints
            .iter()
            .any(|e| e.id == *id && e.direction == direction)
        {
            Ok(())
        } else {
            Err(IoError::NotFound(id.0.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi_forge_core::{
        PortId, Timestamp, packed_short_from_ump, panic_packets, ump_from_packed_short,
    };

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

    #[test]
    fn poll_returns_injected_note_on() {
        let mut backend = NullBackend::with_fixture_ports();
        let id = EndpointId("null:in:0".into());
        backend.open_input(&id, PortId(1)).unwrap();
        let packet = ump_from_packed_short(0x90 | (60 << 8) | (127 << 16));
        backend.inject(MidiEvent::new(
            Timestamp::from_nanos(5_000_000),
            PortId(1),
            packet,
        ));
        let mut out = Vec::new();
        backend.poll(&mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].packet, packet);
        assert_eq!(out[0].time.nanos, 5_000_000);
    }

    #[test]
    fn send_records_panic_packets() {
        let mut backend = NullBackend::with_fixture_ports();
        let id = EndpointId("null:out:0".into());
        backend.open_output(&id, PortId(2)).unwrap();
        let packets = panic_packets();
        for p in &packets {
            backend.send(&id, p).unwrap();
        }
        assert_eq!(backend.sent().len(), 48);
        assert_eq!(
            packed_short_from_ump(&backend.sent()[0].1).unwrap() & 0xFF,
            0xB0
        );
    }
}
