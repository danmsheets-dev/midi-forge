use std::collections::{HashMap, HashSet};

use midi_forge_core::{
    ClockHealth, HangTracker, LiveView, MidiEvent, MonitorLog, MpeTracker, PortId, Router,
    Timestamp, UmpMessage, decode,
};
use midi_forge_io::{Direction, Endpoint, EndpointId, MidiBackend, NullBackend};

/// Underscore-grouped hex of `packet.words()`, e.g. `2090_3C64`.
pub fn format_ump_words(packet: &UmpMessage) -> String {
    packet
        .words()
        .iter()
        .map(|w| format!("{:04X}_{:04X}", w >> 16, w & 0xFFFF))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndpointInfo {
    pub id: String,
    pub name: String,
    pub direction: String,
    pub protocol: String,
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonitorRow {
    pub time_ns: u64,
    pub port: String,
    pub ump_words: String,
    pub decoded: String,
}

/// Session the technician tools read and (when armed) write.
pub trait McpHost {
    fn armed(&self) -> bool;
    fn set_armed(&mut self, armed: bool);
    fn list_endpoints(&self) -> Vec<EndpointInfo>;
    fn monitor_tail(&self, limit: usize) -> Vec<MonitorRow>;
    fn live_dump(&self) -> String;
    fn clock_summary(&self) -> String;
    fn stuck_notes(&self) -> Vec<String>;
    fn thru_graph(&self) -> String;
    fn mpe_status(&self) -> String;
    fn snapshot(&self) -> String;
    fn send(&mut self, dest: &str, packet: &UmpMessage) -> Result<(), String>;
    fn send_sysex(&mut self, dest: &str, bytes: &[u8]) -> Result<(), String>;
    fn set_port_open(&mut self, id: &str, output: bool, open: bool) -> Result<(), String>;
    fn open_outputs(&self) -> Vec<String>;
}

/// Headless host backed by [`NullBackend`] fixtures. Live GUI host is a later task.
pub struct StandaloneHost {
    backend: NullBackend,
    log: MonitorLog,
    live: LiveView,
    clock: ClockHealth,
    hang: HangTracker,
    mpe: MpeTracker,
    router: Router,
    armed: bool,
    open_inputs: HashSet<String>,
    open_outputs: HashSet<String>,
    port_by_endpoint: HashMap<String, PortId>,
    port_names: HashMap<PortId, String>,
    next_port: u32,
}

impl StandaloneHost {
    pub fn with_null() -> Self {
        let mut backend = NullBackend::with_fixture_ports();
        let _ = backend.refresh();
        let mut host = Self {
            backend,
            log: MonitorLog::default(),
            live: LiveView::new(),
            clock: ClockHealth::new(),
            hang: HangTracker::new(),
            mpe: MpeTracker::new(),
            router: Router::new(),
            armed: false,
            open_inputs: HashSet::new(),
            open_outputs: HashSet::new(),
            port_by_endpoint: HashMap::new(),
            port_names: HashMap::new(),
            next_port: 1,
        };
        let ids: Vec<EndpointId> = host
            .backend
            .endpoints()
            .iter()
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            host.ensure_port(&id);
        }
        host
    }

    /// Inject a MIDI 1 Note On (60 / vel 100) into log, live, and hang trackers.
    pub fn push_note(&mut self) {
        let packet = UmpMessage::midi1_channel_voice(0, 0x90, 60, 100);
        let in_id = self
            .backend
            .endpoints()
            .iter()
            .find(|e| e.direction == Direction::Input)
            .map(|e| e.id.0.clone());
        let port = in_id
            .as_ref()
            .and_then(|id| self.port_by_endpoint.get(id).copied())
            .unwrap_or(PortId(0));
        let event = MidiEvent::new(Timestamp::from_nanos(0), port, packet);
        self.log.push(event);
        self.live.push(&packet);
        self.hang.push(&packet);
        self.mpe.push(&packet);
        self.clock.push(&packet, event.time.nanos);
    }

    pub fn sent(&self) -> &[(String, UmpMessage)] {
        self.backend.sent()
    }

    pub fn sent_sysex(&self) -> &[(String, Vec<u8>)] {
        self.backend.sent_sysex()
    }

    fn ensure_port(&mut self, id: &EndpointId) -> PortId {
        if let Some(&port) = self.port_by_endpoint.get(&id.0) {
            return port;
        }
        let port = PortId(self.next_port);
        self.next_port += 1;
        self.port_by_endpoint.insert(id.0.clone(), port);
        let name = self
            .backend
            .endpoints()
            .iter()
            .find(|e| e.id == *id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| id.0.clone());
        self.port_names.insert(port, name);
        port
    }

    #[cfg(test)]
    pub(crate) fn add_named_loopback(&mut self, name: &str) {
        let (a, b) = self.backend.create_loopback(name).expect("loopback");
        self.ensure_port(&a);
        self.ensure_port(&b);
    }

    #[cfg(test)]
    pub(crate) fn relabel_ports(&mut self, name: &str) {
        for label in self.port_names.values_mut() {
            *label = name.to_string();
        }
    }

    fn require_armed(&self) -> Result<(), String> {
        if self.armed {
            Ok(())
        } else {
            Err("writes disabled until arm".into())
        }
    }

    fn find_ep(&self, needle: &str, outputs: bool) -> Result<Endpoint, String> {
        require_dest(needle)?;
        let n = needle.to_ascii_lowercase();
        let eps = self.backend.endpoints();
        if let Some(ep) = eps.iter().find(|e| e.id.0 == needle) {
            return Ok(ep.clone());
        }
        eps.iter()
            .filter(|e| match (outputs, e.direction) {
                (true, Direction::Output | Direction::Bidirectional) => true,
                (false, Direction::Input | Direction::Bidirectional) => true,
                _ => false,
            })
            .find(|e| {
                e.name.to_ascii_lowercase().contains(&n) || e.id.0.to_ascii_lowercase().contains(&n)
            })
            .cloned()
            .ok_or_else(|| {
                let dir = if outputs { "output" } else { "input" };
                format!("no {dir} matching {needle:?}")
            })
    }

    fn find_any(&self, needle: &str) -> Result<Endpoint, String> {
        require_dest(needle)?;
        let n = needle.to_ascii_lowercase();
        self.backend
            .endpoints()
            .iter()
            .find(|e| {
                e.id.0 == needle
                    || e.name.to_ascii_lowercase().contains(&n)
                    || e.id.0.to_ascii_lowercase().contains(&n)
            })
            .cloned()
            .ok_or_else(|| format!("no endpoint matching {needle:?}"))
    }

    fn ensure_output_open(&mut self, id: &EndpointId) -> Result<(), String> {
        if self.open_outputs.contains(&id.0) {
            return Ok(());
        }
        let port = self.ensure_port(id);
        self.backend
            .open_output(id, port)
            .map_err(|e| e.to_string())?;
        self.open_outputs.insert(id.0.clone());
        Ok(())
    }

    fn port_label(&self, port: PortId) -> String {
        self.port_names
            .get(&port)
            .cloned()
            .unwrap_or_else(|| format!("port {}", port.0))
    }

    fn is_open(&self, ep: &Endpoint) -> bool {
        match ep.direction {
            Direction::Input => self.open_inputs.contains(&ep.id.0),
            Direction::Output => self.open_outputs.contains(&ep.id.0),
            Direction::Bidirectional => {
                self.open_inputs.contains(&ep.id.0) || self.open_outputs.contains(&ep.id.0)
            }
        }
    }
}

fn require_dest(needle: &str) -> Result<(), String> {
    if needle.trim().is_empty() {
        Err("empty destination".into())
    } else {
        Ok(())
    }
}

fn direction_label(d: Direction) -> &'static str {
    match d {
        Direction::Input => "in",
        Direction::Output => "out",
        Direction::Bidirectional => "bidi",
    }
}

fn filter_off_flags(filter: &midi_forge_core::Filter) -> Vec<&'static str> {
    let mut off = Vec::new();
    if !filter.notes {
        off.push("notes");
    }
    if !filter.poly_pressure {
        off.push("poly_pressure");
    }
    if !filter.control_change {
        off.push("control_change");
    }
    if !filter.program_change {
        off.push("program_change");
    }
    if !filter.channel_pressure {
        off.push("channel_pressure");
    }
    if !filter.pitch_bend {
        off.push("pitch_bend");
    }
    if !filter.sysex {
        off.push("sysex");
    }
    if !filter.sysex8 {
        off.push("sysex8");
    }
    if !filter.clock {
        off.push("clock");
    }
    if !filter.transport {
        off.push("transport");
    }
    if !filter.active_sensing {
        off.push("active_sensing");
    }
    if !filter.reset {
        off.push("reset");
    }
    if !filter.system_common {
        off.push("system_common");
    }
    if !filter.other {
        off.push("other");
    }
    if !filter.per_note {
        off.push("per_note");
    }
    if !filter.utility {
        off.push("utility");
    }
    if !filter.flex {
        off.push("flex");
    }
    if !filter.stream {
        off.push("stream");
    }
    off
}

impl McpHost for StandaloneHost {
    fn armed(&self) -> bool {
        self.armed
    }

    fn set_armed(&mut self, armed: bool) {
        self.armed = armed;
    }

    fn list_endpoints(&self) -> Vec<EndpointInfo> {
        self.backend
            .endpoints()
            .iter()
            .map(|e| EndpointInfo {
                id: e.id.0.clone(),
                name: e.name.clone(),
                direction: direction_label(e.direction).into(),
                protocol: e.protocol.label().into(),
                open: self.is_open(e),
            })
            .collect()
    }

    fn monitor_tail(&self, limit: usize) -> Vec<MonitorRow> {
        let n = self.log.len();
        let start = n.saturating_sub(limit);
        (start..n)
            .filter_map(|i| {
                let ev = self.log.get(i)?;
                Some(MonitorRow {
                    time_ns: ev.time.nanos,
                    port: self.port_label(ev.port),
                    ump_words: format_ump_words(&ev.packet),
                    decoded: decode(&ev.packet).summary(),
                })
            })
            .collect()
    }

    fn live_dump(&self) -> String {
        self.live.dump()
    }

    fn clock_summary(&self) -> String {
        self.clock.summary()
    }

    fn stuck_notes(&self) -> Vec<String> {
        self.hang
            .notes()
            .into_iter()
            .map(|n| format!("Ch{} note {}", n.channel + 1, n.note))
            .collect()
    }

    fn thru_graph(&self) -> String {
        let links = self.router.links();
        if links.is_empty() {
            return "none".into();
        }
        let mut lines = Vec::new();
        for link in links {
            let from = self.port_label(link.from);
            let to = self.port_label(link.to);
            let off = filter_off_flags(&link.filter);
            if off.is_empty() {
                lines.push(format!("{from} → {to}"));
            } else {
                lines.push(format!("{from} → {to}  ({} off)", off.join(", ")));
            }
        }
        lines.join("\n")
    }

    fn mpe_status(&self) -> String {
        let mut s = self.mpe.mode_summary();
        s.push('\n');
        if self.mpe.voices().is_empty() {
            s.push_str("No sounding MPE notes.");
        } else {
            for v in self.mpe.voices() {
                s.push_str(&format!(
                    "Ch{} note {} vel {}\n",
                    v.channel + 1,
                    v.note,
                    v.velocity
                ));
            }
        }
        s
    }

    fn snapshot(&self) -> String {
        let mut s = String::from("Midi-Forge snapshot\n");
        s.push_str(&format!("backend: {}\n", self.backend.name()));
        s.push_str(&self.clock.summary());
        s.push('\n');
        s.push_str(&self.live.dump());
        if self.hang.is_empty() {
            s.push_str("Stuck notes: none\n");
        } else {
            s.push_str("Stuck notes:\n");
            for line in self.stuck_notes() {
                s.push_str("  ");
                s.push_str(&line);
                s.push('\n');
            }
        }
        s.push_str("Thru:\n  ");
        s.push_str(&self.thru_graph());
        s.push('\n');
        s.push_str(&self.mpe_status());
        s.push('\n');
        s
    }

    fn send(&mut self, dest: &str, packet: &UmpMessage) -> Result<(), String> {
        self.require_armed()?;
        let ep = self.find_ep(dest, true)?;
        self.ensure_output_open(&ep.id)?;
        self.hang.push(packet);
        self.live.push(packet);
        self.backend.send(&ep.id, packet).map_err(|e| e.to_string())
    }

    fn send_sysex(&mut self, dest: &str, bytes: &[u8]) -> Result<(), String> {
        self.require_armed()?;
        let ep = self.find_ep(dest, true)?;
        self.ensure_output_open(&ep.id)?;
        self.backend
            .send_sysex(&ep.id, bytes)
            .map_err(|e| e.to_string())
    }

    fn set_port_open(&mut self, id: &str, output: bool, open: bool) -> Result<(), String> {
        self.require_armed()?;
        let ep = self.find_any(id)?;
        if output {
            if ep.direction == Direction::Input {
                return Err(format!("{} is an input", ep.name));
            }
            if open {
                self.ensure_output_open(&ep.id)?;
            } else if self.open_outputs.remove(&ep.id.0) {
                self.backend
                    .close_output(&ep.id)
                    .map_err(|e| e.to_string())?;
            }
        } else {
            if ep.direction == Direction::Output {
                return Err(format!("{} is an output", ep.name));
            }
            if open {
                if !self.open_inputs.contains(&ep.id.0) {
                    let port = self.ensure_port(&ep.id);
                    self.backend
                        .open_input(&ep.id, port)
                        .map_err(|e| e.to_string())?;
                    self.open_inputs.insert(ep.id.0.clone());
                }
            } else if self.open_inputs.remove(&ep.id.0) {
                self.backend
                    .close_input(&ep.id)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn open_outputs(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.open_outputs.iter().cloned().collect();
        ids.sort();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi_forge_core::IDENTITY_REQUEST;

    #[test]
    fn writes_fail_when_unarmed() {
        let mut host = StandaloneHost::with_null();
        let packet = UmpMessage::midi1_channel_voice(0, 0x90, 60, 100);
        let send_err = host.send("null:out:0", &packet).unwrap_err();
        assert!(send_err.to_lowercase().contains("arm"));
        assert!(host.sent().is_empty());

        let sysex_err = host
            .send_sysex("null:out:0", &IDENTITY_REQUEST)
            .unwrap_err();
        assert!(sysex_err.to_lowercase().contains("arm"));
        assert!(host.sent_sysex().is_empty());

        let port_err = host.set_port_open("null:out:0", true, true).unwrap_err();
        assert!(port_err.to_lowercase().contains("arm"));
        assert!(host.open_outputs().is_empty());
    }

    #[test]
    fn writes_reject_empty_destination() {
        let mut host = StandaloneHost::with_null();
        host.set_armed(true);
        let packet = UmpMessage::midi1_channel_voice(0, 0x90, 60, 100);
        assert!(host.send("", &packet).is_err());
        assert!(host.send("   ", &packet).is_err());
        assert!(host.sent().is_empty());
        assert!(host.send_sysex("", &IDENTITY_REQUEST).is_err());
        assert!(host.send_sysex("\t", &IDENTITY_REQUEST).is_err());
        assert!(host.sent_sysex().is_empty());
        assert!(host.set_port_open("", true, true).is_err());
        assert!(host.set_port_open(" \t", false, true).is_err());
        assert!(host.open_outputs().is_empty());
        assert!(host.open_inputs.is_empty());
    }
}
