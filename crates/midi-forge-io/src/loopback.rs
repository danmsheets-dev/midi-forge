use midi_forge_core::{Midi1Parser, MidiEvent, PortId, Timestamp, UmpMessage};

use crate::backend::{Direction, Endpoint, EndpointId, ProtocolHint};
use crate::error::IoError;

/// In-process A/B MIDI cable. Other applications do not see these ports.
pub struct SoftwareLoopbacks {
    cables: Vec<Cable>,
    next_index: u32,
}

struct Cable {
    index: u32,
    name: String,
    in_port: Option<PortId>,
    out_open: bool,
    queue: Vec<MidiEvent>,
}

impl SoftwareLoopbacks {
    pub fn new() -> Self {
        Self {
            cables: Vec::new(),
            next_index: 1,
        }
    }

    pub fn create(&mut self, name: &str) -> (EndpointId, EndpointId) {
        let index = self.next_index;
        self.next_index += 1;
        let label = if name.trim().is_empty() {
            format!("Forge Cable {index}")
        } else {
            name.trim().to_string()
        };
        self.cables.push(Cable {
            index,
            name: label,
            in_port: None,
            out_open: false,
            queue: Vec::new(),
        });
        (in_id(index), out_id(index))
    }

    pub fn remove(&mut self, id: &EndpointId) -> Result<(), IoError> {
        let index = parse_index(&id.0).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        let before = self.cables.len();
        self.cables.retain(|c| c.index != index);
        if self.cables.len() == before {
            return Err(IoError::NotFound(id.0.clone()));
        }
        Ok(())
    }

    pub fn endpoints(&self) -> Vec<Endpoint> {
        let mut eps = Vec::with_capacity(self.cables.len() * 2);
        for c in &self.cables {
            eps.push(Endpoint {
                id: in_id(c.index),
                name: format!("{} In", c.name),
                direction: Direction::Input,
                protocol: ProtocolHint::Midi1Bytes,
            });
            eps.push(Endpoint {
                id: out_id(c.index),
                name: format!("{} Out", c.name),
                direction: Direction::Output,
                protocol: ProtocolHint::Midi1Bytes,
            });
        }
        eps
    }

    pub fn is_ours(&self, id: &EndpointId) -> bool {
        parse_index(&id.0).is_some()
    }

    pub fn open_input(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError> {
        let cable = self.cable_mut(id)?;
        if !id.0.contains(":in") {
            return Err(IoError::NotFound(id.0.clone()));
        }
        if cable.in_port.is_some() {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        cable.in_port = Some(port);
        Ok(())
    }

    pub fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if let Ok(cable) = self.cable_mut(id) {
            cable.in_port = None;
            cable.queue.clear();
        }
        Ok(())
    }

    pub fn open_output(&mut self, id: &EndpointId) -> Result<(), IoError> {
        let cable = self.cable_mut(id)?;
        if !id.0.contains(":out") {
            return Err(IoError::NotFound(id.0.clone()));
        }
        if cable.out_open {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        cable.out_open = true;
        Ok(())
    }

    pub fn close_output(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if let Ok(cable) = self.cable_mut(id) {
            cable.out_open = false;
        }
        Ok(())
    }

    pub fn send(&mut self, id: &EndpointId, packet: UmpMessage) -> Result<(), IoError> {
        let cable = self.cable_mut(id)?;
        if !cable.out_open {
            return Err(IoError::NotFound(id.0.clone()));
        }
        if let Some(port) = cable.in_port {
            cable
                .queue
                .push(MidiEvent::new(Timestamp::from_nanos(0), port, packet));
        }
        Ok(())
    }

    pub fn send_sysex(&mut self, id: &EndpointId, bytes: &[u8]) -> Result<(), IoError> {
        let mut parser = Midi1Parser::new();
        let packets = parser.push_slice(bytes);
        for p in packets {
            self.send(id, p)?;
        }
        Ok(())
    }

    pub fn poll(&mut self, out: &mut Vec<MidiEvent>) {
        for cable in &mut self.cables {
            out.append(&mut cable.queue);
        }
    }

    fn cable_mut(&mut self, id: &EndpointId) -> Result<&mut Cable, IoError> {
        let index = parse_index(&id.0).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        self.cables
            .iter_mut()
            .find(|c| c.index == index)
            .ok_or_else(|| IoError::NotFound(id.0.clone()))
    }
}

impl Default for SoftwareLoopbacks {
    fn default() -> Self {
        Self::new()
    }
}

fn in_id(index: u32) -> EndpointId {
    EndpointId(format!("forge:loop:{index}:in"))
}

fn out_id(index: u32) -> EndpointId {
    EndpointId(format!("forge:loop:{index}:out"))
}

fn parse_index(id: &str) -> Option<u32> {
    let rest = id.strip_prefix("forge:loop:")?;
    let (num, _) = rest.split_once(':')?;
    num.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi_forge_core::ump_from_status_data;

    #[test]
    fn send_to_out_appears_on_in_when_open() {
        let mut loops = SoftwareLoopbacks::new();
        let (inp, outp) = loops.create("Test");
        loops.open_input(&inp, PortId(9)).unwrap();
        loops.open_output(&outp).unwrap();
        let note = ump_from_status_data(0x90, 60, 100);
        loops.send(&outp, note).unwrap();
        let mut got = Vec::new();
        loops.poll(&mut got);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].port, PortId(9));
        assert_eq!(got[0].packet, note);
    }

    #[test]
    fn send_dropped_if_input_closed() {
        let mut loops = SoftwareLoopbacks::new();
        let (_inp, outp) = loops.create("Test");
        loops.open_output(&outp).unwrap();
        loops.send(&outp, ump_from_status_data(0x90, 1, 1)).unwrap();
        let mut got = Vec::new();
        loops.poll(&mut got);
        assert!(got.is_empty());
    }
}
