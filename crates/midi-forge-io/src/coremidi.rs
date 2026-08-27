//! CoreMIDI backend (macOS). Virtual ports are visible to other apps.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use coremidi::{
    Client, Destination, Destinations, EventBuffer, EventList, InputPortWithContext, OutputPort,
    Protocol, Source, Sources,
};

use midi_forge_core::{Midi1Parser, MidiEvent, PortId, Timestamp, UmpMessage};

use crate::backend::{Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint};
use crate::error::IoError;

const QUEUE_CAP: usize = 4096;

#[derive(Clone, Copy)]
enum CaptureKey {
    Port(PortId),
    Virtual(u32),
}

struct OpenIn {
    _port: InputPortWithContext<()>,
    id: String,
}

pub struct CoreMidiBackend {
    endpoints: Vec<Endpoint>,
    client: Option<Client>,
    output_port: Option<OutputPort>,
    inputs: Vec<OpenIn>,
    rx: Receiver<(CaptureKey, UmpMessage)>,
    tx: SyncSender<(CaptureKey, UmpMessage)>,
    dropped: Arc<AtomicU64>,
    virtual_sources: Vec<(u32, coremidi::VirtualSource)>,
    virtual_dests: Vec<(u32, coremidi::VirtualDestination, Arc<AtomicBool>)>,
    next_virtual: u32,
    virtual_in_ports: HashMap<u32, PortId>,
}

impl CoreMidiBackend {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(QUEUE_CAP);
        let client = Client::new("Midi-Forge").ok();
        let output_port = client
            .as_ref()
            .and_then(|c| c.output_port("Midi-Forge out").ok());
        let mut this = Self {
            endpoints: Vec::new(),
            client,
            output_port,
            inputs: Vec::new(),
            rx,
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
            virtual_sources: Vec::new(),
            virtual_dests: Vec::new(),
            next_virtual: 1,
            virtual_in_ports: HashMap::new(),
        };
        let _ = this.refresh();
        this
    }
}

impl Default for CoreMidiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiBackend for CoreMidiBackend {
    fn name(&self) -> &'static str {
        "coremidi"
    }

    fn refresh(&mut self) -> Result<(), IoError> {
        self.rebuild_endpoints();
        Ok(())
    }

    fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    fn open_input(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError> {
        if let Some(idx) = parse_suffix(&id.0, "coremidi:vd:") {
            let dest = self
                .virtual_dests
                .iter()
                .find(|(n, _, _)| *n == idx)
                .ok_or_else(|| IoError::NotFound(id.0.clone()))?;
            if self.virtual_in_ports.contains_key(&idx) {
                return Err(IoError::AlreadyOpen(id.0.clone()));
            }
            dest.2.store(true, Ordering::Release);
            self.virtual_in_ports.insert(idx, port);
            return Ok(());
        }

        if self.inputs.iter().any(|p| p.id == id.0) {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        let i = parse_index(&id.0, "coremidi:src:")?;
        let src = Source::from_index(i).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        let tx = self.tx.clone();
        let dropped = Arc::clone(&self.dropped);
        let mut input = self
            .client
            .as_ref()
            .ok_or_else(|| IoError::Backend("CoreMIDI client unavailable".into()))?
            .input_port_with_protocol(
                &format!("Midi-Forge in {i}"),
                Protocol::Midi20,
                move |event_list, _: &mut ()| {
                    push_event_list(&tx, &dropped, CaptureKey::Port(port), event_list);
                },
            )
            .map_err(|e| IoError::Backend(format!("input port: {e}")))?;
        input
            .connect_source(&src, ())
            .map_err(|e| IoError::Backend(format!("connect: {e}")))?;
        self.inputs.push(OpenIn {
            _port: input,
            id: id.0.clone(),
        });
        Ok(())
    }

    fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if let Some(idx) = parse_suffix(&id.0, "coremidi:vd:") {
            self.virtual_in_ports.remove(&idx);
            if let Some((_, _, armed)) = self.virtual_dests.iter().find(|(n, _, _)| *n == idx) {
                armed.store(false, Ordering::Release);
            }
            return Ok(());
        }
        self.inputs.retain(|p| p.id != id.0);
        Ok(())
    }

    fn open_output(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        if parse_index(&id.0, "coremidi:dst:").is_ok()
            || parse_suffix(&id.0, "coremidi:vs:").is_some()
        {
            return Ok(());
        }
        Err(IoError::NotFound(id.0.clone()))
    }

    fn close_output(&mut self, _id: &EndpointId) -> Result<(), IoError> {
        Ok(())
    }

    fn poll(&mut self, out: &mut Vec<MidiEvent>) -> u64 {
        while let Ok((key, packet)) = self.rx.try_recv() {
            let port = match key {
                CaptureKey::Port(p) => p,
                CaptureKey::Virtual(idx) => match self.virtual_in_ports.get(&idx) {
                    Some(&p) => p,
                    None => continue,
                },
            };
            out.push(MidiEvent::new(Timestamp::from_nanos(0), port, packet));
        }
        self.dropped.load(Ordering::Relaxed)
    }

    fn send(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError> {
        for packet in midi_forge_core::downscale_to_midi1(packet) {
            self.send_one(id, &packet)?;
        }
        Ok(())
    }

    fn send_sysex(&mut self, id: &EndpointId, bytes: &[u8]) -> Result<(), IoError> {
        if bytes.first() != Some(&0xF0) || bytes.last() != Some(&0xF7) {
            return Err(IoError::UnsupportedPacket);
        }
        let mut parser = Midi1Parser::new();
        for packet in parser.push_slice(bytes) {
            self.send(id, &packet)?;
        }
        Ok(())
    }

    fn create_loopback(&mut self, name: &str) -> Result<(EndpointId, EndpointId), IoError> {
        let idx = self.next_virtual;
        self.next_virtual += 1;
        let label = if name.trim().is_empty() {
            format!("Midi-Forge {idx}")
        } else {
            name.trim().to_string()
        };
        let tx = self.tx.clone();
        let dropped = Arc::clone(&self.dropped);
        let armed = Arc::new(AtomicBool::new(false));
        let armed_cb = Arc::clone(&armed);
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| IoError::Backend("CoreMIDI client unavailable".into()))?;
        let source = client
            .virtual_source(&format!("{label} Out"))
            .map_err(|e| IoError::Backend(format!("virtual source: {e}")))?;
        let dest = client
            .virtual_destination_with_protocol(
                &format!("{label} In"),
                Protocol::Midi20,
                move |event_list| {
                    if !armed_cb.load(Ordering::Acquire) {
                        return;
                    }
                    push_event_list(&tx, &dropped, CaptureKey::Virtual(idx), event_list);
                },
            )
            .map_err(|e| IoError::Backend(format!("virtual dest: {e}")))?;
        self.virtual_sources.push((idx, source));
        self.virtual_dests.push((idx, dest, armed));
        self.rebuild_endpoints();
        Ok((
            EndpointId(format!("coremidi:vd:{idx}")),
            EndpointId(format!("coremidi:vs:{idx}")),
        ))
    }

    fn remove_loopback(&mut self, id: &EndpointId) -> Result<(), IoError> {
        let idx: u32 =
            id.0.rsplit(':')
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        let before = self.virtual_sources.len() + self.virtual_dests.len();
        self.virtual_sources.retain(|(n, _)| *n != idx);
        self.virtual_dests.retain(|(n, _, _)| *n != idx);
        self.virtual_in_ports.remove(&idx);
        if self.virtual_sources.len() + self.virtual_dests.len() == before {
            return Err(IoError::NotFound(id.0.clone()));
        }
        self.rebuild_endpoints();
        Ok(())
    }

    fn caps(&self) -> crate::backend::BackendCaps {
        crate::backend::BackendCaps {
            native_ump: true,
            scheduled_send: false,
            daw_visible_virtual: true,
            multi_client: true,
        }
    }
}

impl CoreMidiBackend {
    fn send_one(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError> {
        let buf = event_buffer(packet);
        if let Ok(i) = parse_index(&id.0, "coremidi:dst:") {
            let dest = Destination::from_index(i).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
            let port = self
                .output_port
                .as_ref()
                .ok_or_else(|| IoError::Backend("no output port".into()))?;
            port.send(&dest, &buf)
                .map_err(|e| IoError::Backend(format!("send: {e}")))?;
            return Ok(());
        }
        if let Some(idx) = parse_suffix(&id.0, "coremidi:vs:") {
            let src = self
                .virtual_sources
                .iter()
                .find(|(n, _)| *n == idx)
                .ok_or_else(|| IoError::NotFound(id.0.clone()))?;
            src.1
                .received(&buf)
                .map_err(|e| IoError::Backend(format!("virtual send: {e}")))?;
            return Ok(());
        }
        Err(IoError::NotFound(id.0.clone()))
    }

    fn rebuild_endpoints(&mut self) {
        let owned = self.owned_unique_ids();
        let mut endpoints = Vec::new();
        for (i, src) in Sources.into_iter().enumerate() {
            if skip_owned(&owned, src.unique_id()) {
                continue;
            }
            let name = src.display_name().unwrap_or_else(|| format!("Source {i}"));
            endpoints.push(Endpoint {
                id: EndpointId(format!("coremidi:src:{i}")),
                name,
                direction: Direction::Input,
                protocol: ProtocolHint::Ump,
            });
        }
        for (i, dst) in Destinations.into_iter().enumerate() {
            if skip_owned(&owned, dst.unique_id()) {
                continue;
            }
            let name = dst.display_name().unwrap_or_else(|| format!("Dest {i}"));
            endpoints.push(Endpoint {
                id: EndpointId(format!("coremidi:dst:{i}")),
                name,
                direction: Direction::Output,
                protocol: ProtocolHint::Ump,
            });
        }
        for (idx, src) in &self.virtual_sources {
            let name = src.display_name().unwrap_or_else(|| format!("Forge {idx}"));
            endpoints.push(Endpoint {
                id: EndpointId(format!("coremidi:vs:{idx}")),
                name: format!("{name} (virtual out)"),
                direction: Direction::Output,
                protocol: ProtocolHint::Ump,
            });
        }
        for (idx, dst, _) in &self.virtual_dests {
            let name = dst.display_name().unwrap_or_else(|| format!("Forge {idx}"));
            endpoints.push(Endpoint {
                id: EndpointId(format!("coremidi:vd:{idx}")),
                name: format!("{name} (virtual in)"),
                direction: Direction::Input,
                protocol: ProtocolHint::Ump,
            });
        }
        self.endpoints = endpoints;
    }

    fn owned_unique_ids(&self) -> HashSet<u32> {
        let mut ids = HashSet::new();
        for (_, src) in &self.virtual_sources {
            if let Some(id) = src.unique_id() {
                ids.insert(id);
            }
        }
        for (_, dst, _) in &self.virtual_dests {
            if let Some(id) = dst.unique_id() {
                ids.insert(id);
            }
        }
        ids
    }
}

fn skip_owned(owned: &HashSet<u32>, unique: Option<u32>) -> bool {
    unique.is_some_and(|id| owned.contains(&id))
}

fn push_event_list(
    tx: &SyncSender<(CaptureKey, UmpMessage)>,
    dropped: &AtomicU64,
    key: CaptureKey,
    event_list: &EventList,
) {
    for packet in event_list.iter() {
        if let Ok(ump) = UmpMessage::try_from_words(packet.data()) {
            match tx.try_send((key, ump)) {
                Ok(()) => {}
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

fn event_buffer(packet: &UmpMessage) -> EventBuffer {
    let proto = if packet.message_type() >= 0x4 {
        Protocol::Midi20
    } else {
        Protocol::Midi10
    };
    EventBuffer::new(proto).with_packet(0, packet.words())
}

fn parse_index(id: &str, prefix: &str) -> Result<usize, IoError> {
    id.strip_prefix(prefix)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| IoError::NotFound(id.to_string()))
}

fn parse_suffix(id: &str, prefix: &str) -> Option<u32> {
    id.strip_prefix(prefix)?.parse().ok()
}
