use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use eframe::egui;
use midi_forge_core::{
    ClockHealth, ClockMaster, HangTracker, LiveView, MessageKind, MidiEvent, MonitorLog,
    MpeTracker, NrpnTracker, PortId, Profile, ProfileLink, RouteEvent, RouteLog, Router, Scene,
    SessionRecorder, SysexAssembler, UmpMessage, decode, format_wire_hex, message_kind,
    panic_packets,
};
use midi_forge_io::{
    Direction, Endpoint, EndpointId, MidiBackend, NetUmp, default_backend, explain_in_use,
    probe_wms,
};

use crate::clock;
use crate::inject;
use crate::live;
use crate::mpe;
use crate::script::{self, RightTab};
use crate::sysex::{self, Librarian};
use crate::thru;

pub struct MidiForgeApp {
    inner: Arc<Mutex<EngineInner>>,
    stop: Arc<AtomicBool>,
}

/// MIDI + UI state. The engine thread `try_lock`s this to tick; the UI
/// `lock`s it while drawing and editing.
pub(crate) struct EngineInner {
    pub(crate) backend: Box<dyn MidiBackend>,
    backend_name: String,
    pub(crate) endpoints: Vec<Endpoint>,
    log: MonitorLog,
    pub(crate) router: Router,
    paused: bool,
    follow: bool,
    dropped: u64,
    open_inputs: HashSet<String>,
    pub(crate) open_outputs: HashSet<String>,
    pub(crate) port_names: HashMap<PortId, String>,
    port_by_endpoint: HashMap<String, PortId>,
    endpoint_by_port: HashMap<PortId, EndpointId>,
    pub(crate) port_errors: HashMap<String, String>,
    pub(crate) selected_link: Option<(PortId, PortId)>,
    next_port: u32,
    capture_buf: Vec<MidiEvent>,
    pub(crate) status: String,
    pub(crate) librarian: Librarian,
    thru_sysex: HashMap<String, SysexAssembler>,
    pub(crate) mpe: MpeTracker,
    pub(crate) mpe_members: u8,
    pub(crate) cable_name: String,
    pub(crate) script: midi_forge_script::ScriptEngine,
    pub(crate) right_tab: RightTab,
    pub(crate) hang: HangTracker,
    pub(crate) live: LiveView,
    pub(crate) nrpn: NrpnTracker,
    pub(crate) wms_note: String,
    pub(crate) mute_clock: bool,
    pub(crate) inject_channel: u8,
    pub(crate) inject_octave: i8,
    pub(crate) inject_velocity: u8,
    pub(crate) inject_cc: u8,
    pub(crate) inject_cc_val: u8,
    pub(crate) inject_dest: Option<String>,
    pub(crate) held_keys: HashSet<u8>,
    pub(crate) mon_search: String,
    pub(crate) mon_notes: bool,
    pub(crate) mon_cc: bool,
    pub(crate) mon_clock: bool,
    pub(crate) mon_sysex: bool,
    pub(crate) mon_other: bool,
    pub(crate) mon_channel: u8,
    activity: HashMap<String, Instant>,
    last_hotplug: Instant,
    device_fp: String,
    pub(crate) throttle_ms: u32,
    throttle_q: HashMap<String, VecDeque<UmpMessage>>,
    throttle_at: HashMap<String, Instant>,
    pub(crate) clock: ClockHealth,
    pub(crate) routes: RouteLog,
    pub(crate) learn: Option<(PortId, PortId)>,
    pub(crate) always_on_top: bool,
    host_epoch: Instant,
    pub(crate) scene_name: String,
    pub(crate) scenes: Vec<Scene>,
    pub(crate) pack_idx: usize,
    pub(crate) master: ClockMaster,
    pub(crate) master_dest: Option<String>,
    pub(crate) inject_m2: bool,
    pub(crate) recorder: SessionRecorder,
    pub(crate) pe_header: String,
    pub(crate) pe_body: String,
    pub(crate) pe_note: String,
    pub(crate) device_idx: usize,
    pub(crate) net: NetUmp,
}

impl MidiForgeApp {
    pub(crate) fn eng(&self) -> MutexGuard<'_, EngineInner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let inner = Arc::new(Mutex::new(EngineInner::new(cc)));
        let stop = Arc::new(AtomicBool::new(false));
        let worker = Arc::clone(&inner);
        let halt = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("midi-engine".into())
            .spawn(move || engine_loop(worker, halt))
            .expect("midi-engine thread");
        Self { inner, stop }
    }
}

impl Drop for MidiForgeApp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn engine_loop(inner: Arc<Mutex<EngineInner>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        if let Ok(mut g) = inner.try_lock() {
            g.tick();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

impl EngineInner {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut backend: Box<dyn MidiBackend> = default_backend();
        let backend_name = backend.name().to_string();
        let (endpoints, status) = match backend.refresh() {
            Ok(()) => (backend.endpoints().to_vec(), String::new()),
            Err(err) => (Vec::new(), err.to_string()),
        };

        let mut app = Self {
            backend,
            backend_name,
            endpoints,
            log: MonitorLog::default(),
            router: Router::new(),
            paused: false,
            follow: true,
            dropped: 0,
            open_inputs: HashSet::new(),
            open_outputs: HashSet::new(),
            port_names: HashMap::new(),
            port_by_endpoint: HashMap::new(),
            endpoint_by_port: HashMap::new(),
            port_errors: HashMap::new(),
            selected_link: None,
            next_port: 1,
            capture_buf: Vec::new(),
            status,
            librarian: Librarian::new(),
            thru_sysex: HashMap::new(),
            mpe: MpeTracker::new(),
            mpe_members: 15,
            cable_name: "Forge Cable".into(),
            script: midi_forge_script::ScriptEngine::new(),
            right_tab: RightTab::Sysex,
            hang: HangTracker::new(),
            live: LiveView::new(),
            nrpn: NrpnTracker::new(),
            wms_note: probe_wms().summary,
            mute_clock: false,
            inject_channel: 1,
            inject_octave: 0,
            inject_velocity: 100,
            inject_cc: 1,
            inject_cc_val: 0,
            inject_dest: None,
            held_keys: HashSet::new(),
            mon_search: String::new(),
            mon_notes: true,
            mon_cc: true,
            mon_clock: true,
            mon_sysex: true,
            mon_other: true,
            mon_channel: 0,
            activity: HashMap::new(),
            last_hotplug: Instant::now(),
            device_fp: String::new(),
            clock: ClockHealth::new(),
            routes: RouteLog::default(),
            learn: None,
            always_on_top: false,
            host_epoch: Instant::now(),
            scene_name: "Default".into(),
            scenes: Vec::new(),
            pack_idx: 0,
            throttle_ms: 0,
            throttle_q: HashMap::new(),
            throttle_at: HashMap::new(),
            master: ClockMaster::new(),
            master_dest: None,
            inject_m2: false,
            recorder: SessionRecorder::default(),
            pe_header: r#"{"resource":"DeviceInfo"}"#.into(),
            pe_body: String::new(),
            pe_note: String::new(),
            device_idx: 0,
            net: NetUmp::default(),
        };

        let inputs: Vec<EndpointId> = app
            .endpoints
            .iter()
            .filter(|e| e.direction == Direction::Input)
            .map(|e| e.id.clone())
            .collect();
        for id in inputs {
            if let Err(err) = app.set_input_open(&id, true) {
                app.port_errors.insert(id.0, err);
            }
        }
        app.device_fp = device_fingerprint(&app.endpoints);
        app
    }

    pub(crate) fn tick(&mut self) {
        self.drain_capture();
        self.tick_throttle();
        self.poll_hotplug();
        self.tick_master();
        self.tick_script_timers();
        self.tick_net();
    }

    fn tick_master(&mut self) {
        let now = self.host_ns();
        let packets = self.master.poll(now);
        let Some(dest) = self.master_dest.clone() else {
            return;
        };
        if packets.is_empty() {
            return;
        }
        let id = EndpointId(dest);
        if !self.open_outputs.contains(&id.0) {
            let _ = self.set_output_open(&id, true);
        }
        for packet in packets {
            if let Err(err) = self.backend.send(&id, &packet) {
                self.port_errors.insert(id.0.clone(), err.to_string());
                break;
            }
        }
    }

    fn tick_script_timers(&mut self) {
        let now = self.host_ns();
        let extra = self.script.tick(now);
        for event in extra {
            if !self.paused {
                self.log.push(event);
                self.recorder.push(event);
            }
            let routed_list = self.router.route(&event);
            for routed in routed_list {
                let Some(dest) = self.endpoint_by_port.get(&routed.port).cloned() else {
                    continue;
                };
                if !self.open_outputs.contains(&dest.0) {
                    continue;
                }
                self.hang.push(&routed.packet);
                let _ = self.backend.send(&dest, &routed.packet);
            }
        }
    }

    fn tick_net(&mut self) {
        use midi_forge_core::{
            CMD_INVITATION, decode_command, decode_ump, encode_command, looks_like_command,
        };
        let packets = self.net.poll();
        for (from, bytes) in packets {
            self.net.last = format!("{from} {} B", bytes.len());
            if looks_like_command(&bytes) {
                if let Some((cmd, _, _)) = decode_command(&bytes)
                    && cmd == CMD_INVITATION
                {
                    let reply = encode_command(0x10, 0, &[]);
                    let _ = self.net.send_bytes(&reply);
                    self.net.last = format!("accepted invitation from {from}");
                }
                continue;
            }
            let port = self.ensure_port(&EndpointId("net:ump".into()));
            for packet in decode_ump(&bytes) {
                let ev = MidiEvent::new(
                    midi_forge_core::Timestamp::from_nanos(self.host_ns()),
                    port,
                    packet,
                );
                if !self.paused {
                    self.log.push(ev);
                    self.recorder.push(ev);
                }
            }
        }
    }

    pub(crate) fn ensure_port(&mut self, id: &EndpointId) -> PortId {
        if let Some(&port) = self.port_by_endpoint.get(&id.0) {
            return port;
        }
        let port = PortId(self.next_port);
        self.next_port += 1;
        self.port_by_endpoint.insert(id.0.clone(), port);
        self.endpoint_by_port.insert(port, id.clone());
        let name = self
            .endpoints
            .iter()
            .find(|e| e.id == *id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| id.0.clone());
        self.port_names.insert(port, name);
        port
    }

    pub(crate) fn set_input_open(&mut self, id: &EndpointId, open: bool) -> Result<(), String> {
        let port = self.ensure_port(id);
        if open {
            if self.open_inputs.contains(&id.0) {
                return Ok(());
            }
            self.backend
                .open_input(id, port)
                .map_err(|e| self.open_err(id, &e))?;
            self.open_inputs.insert(id.0.clone());
            self.port_errors.remove(&id.0);
        } else if self.open_inputs.remove(&id.0) {
            self.backend.close_input(id).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub(crate) fn set_output_open(&mut self, id: &EndpointId, open: bool) -> Result<(), String> {
        let port = self.ensure_port(id);
        if open {
            if self.open_outputs.contains(&id.0) {
                return Ok(());
            }
            self.backend
                .open_output(id, port)
                .map_err(|e| self.open_err(id, &e))?;
            self.open_outputs.insert(id.0.clone());
            self.port_errors.remove(&id.0);
        } else if self.open_outputs.remove(&id.0) {
            self.backend.close_output(id).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub(crate) fn set_thru(
        &mut self,
        from: &EndpointId,
        to: &EndpointId,
        linked: bool,
    ) -> Result<(), String> {
        if midi_forge_io::is_loopback_pair(&from.0, &to.0) {
            return Err(
                "Refusing loopback In→Out (that recirculates every event). Use two cables.".into(),
            );
        }
        if linked {
            self.set_input_open(from, true)?;
            self.set_output_open(to, true)?;
        }
        let from_port = self.ensure_port(from);
        let to_port = self.ensure_port(to);
        self.router.set_linked(from_port, to_port, linked);
        if linked {
            self.selected_link = Some((from_port, to_port));
        } else if self.selected_link == Some((from_port, to_port)) {
            self.selected_link = None;
        }
        Ok(())
    }

    fn open_err(&self, id: &EndpointId, err: &midi_forge_io::IoError) -> String {
        let name = self
            .endpoints
            .iter()
            .find(|e| e.id == *id)
            .map(|e| e.name.as_str())
            .unwrap_or(&id.0);
        explain_in_use(err, name)
    }

    pub(crate) fn host_ns(&self) -> u64 {
        self.host_epoch.elapsed().as_nanos() as u64
    }

    fn drain_capture(&mut self) {
        self.capture_buf.clear();
        self.dropped = self.backend.poll(&mut self.capture_buf);
        let events: Vec<MidiEvent> = self.capture_buf.drain(..).collect();
        if !self.paused {
            for event in &events {
                self.log.push(*event);
                self.recorder.push(*event);
            }
        }
        let now = Instant::now();
        for event in &events {
            if let Some(ep) = self.endpoint_by_port.get(&event.port) {
                self.activity.insert(ep.0.clone(), now);
            }
            let t_ns = self.host_ns();
            self.clock.push(&event.packet, t_ns);
            self.live.push(&event.packet);
            let _ = self.nrpn.push(&event.packet);
            self.mpe.push(&event.packet);
            self.librarian.on_packet(&event.packet);
            if let Some((from, to)) = self.learn
                && event.port == from
                && let Some(mut map) = self.router.map(from, to).cloned()
                && let Some(label) = map.learn_insert(&event.packet)
            {
                self.router.set_map(from, to, map);
                self.learn = None;
                self.status = format!("Learned {label} — edit the action to remap");
            }
            let processed = self.script.process(event);
            for event in &processed {
                if self.mute_clock {
                    let kind = message_kind(&event.packet);
                    if kind == MessageKind::Clock || kind == MessageKind::ActiveSensing {
                        continue;
                    }
                }
                let routed_list = self.router.route(event);
                let dests: Vec<PortId> = routed_list.iter().map(|r| r.port).collect();
                let kind = message_kind(&event.packet);
                if kind != MessageKind::Clock && kind != MessageKind::ActiveSensing {
                    self.routes.push(RouteEvent {
                        time: event.time,
                        from: event.port,
                        dests,
                        packet: event.packet,
                    });
                }
                for routed in routed_list {
                    let Some(dest) = self.endpoint_by_port.get(&routed.port).cloned() else {
                        continue;
                    };
                    if !self.open_outputs.contains(&dest.0) {
                        continue;
                    }
                    self.hang.push(&routed.packet);
                    if routed.packet.message_type() == 0x3 {
                        let asm = self.thru_sysex.entry(dest.0.clone()).or_default();
                        if let Some(dump) = asm.push(&routed.packet)
                            && let Err(err) = self.backend.send_sysex(&dest, dump.bytes())
                        {
                            self.port_errors.insert(dest.0, err.to_string());
                        }
                    } else if self.throttle_ms > 0 {
                        let q = self.throttle_q.entry(dest.0.clone()).or_default();
                        if q.len() >= 4096 {
                            q.pop_front();
                        }
                        q.push_back(routed.packet);
                    } else if let Err(err) = self.backend.send(&dest, &routed.packet) {
                        self.port_errors.insert(dest.0, err.to_string());
                    }
                }
            }
        }
    }

    pub(crate) fn send_sysex_now(&mut self, id: &EndpointId, bytes: &[u8]) -> Result<(), String> {
        self.backend
            .send_sysex(id, bytes)
            .map_err(|e| e.to_string())
    }

    fn panic_now(&mut self) {
        let outputs: Vec<EndpointId> = self
            .endpoints
            .iter()
            .filter(|e| e.direction == Direction::Output)
            .map(|e| e.id.clone())
            .collect();
        for id in &outputs {
            if let Err(err) = self.set_output_open(id, true) {
                self.port_errors.insert(id.0.clone(), err);
            }
        }

        let packets = panic_packets();
        let mut sent = 0usize;
        let ids: Vec<String> = self.open_outputs.iter().cloned().collect();
        for id in &ids {
            for packet in &packets {
                match self.backend.send(&EndpointId(id.clone()), packet) {
                    Ok(()) => sent += 1,
                    Err(err) => {
                        self.port_errors.insert(id.clone(), err.to_string());
                        break;
                    }
                }
            }
        }
        let hanging = self.hang.note_off_packets();
        for id in &ids {
            for packet in &hanging {
                if self.backend.send(&EndpointId(id.clone()), packet).is_ok() {
                    sent += 1;
                }
            }
        }
        self.hang.clear();
        self.live = LiveView::new();
        self.mpe.clear_voices();
        self.throttle_q.clear();
        self.throttle_at.clear();
        self.status = format!("Panic: sent {sent} short messages to open outputs");
    }

    pub(crate) fn snapshot_text(&self) -> String {
        let mut s = String::from("Midi-Forge snapshot\n");
        s.push_str(&format!("backend: {}\n", self.backend_name));
        s.push_str(&self.wms_note);
        s.push('\n');
        s.push_str(&self.clock.summary());
        s.push('\n');
        s.push_str(&self.live.dump());
        if self.hang.is_empty() {
            s.push_str("Stuck notes: none\n");
        } else {
            s.push_str("Stuck notes:\n");
            for n in self.hang.notes() {
                s.push_str(&format!("  Ch{} note {}\n", n.channel + 1, n.note));
            }
        }
        if let Some(p) = self.nrpn.last() {
            s.push_str(&p.summary());
            s.push('\n');
        }
        s.push_str("Thru path (recent):\n");
        if self.routes.is_empty() {
            s.push_str("  (none)\n");
        } else {
            for ev in self.routes.iter().rev().take(16) {
                let from = self
                    .port_names
                    .get(&ev.from)
                    .cloned()
                    .unwrap_or_else(|| format!("p{}", ev.from.0));
                let dests = if ev.dests.is_empty() {
                    "dropped".into()
                } else {
                    ev.dests
                        .iter()
                        .map(|p| {
                            self.port_names
                                .get(p)
                                .cloned()
                                .unwrap_or_else(|| format!("p{}", p.0))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                s.push_str(&format!(
                    "  {from} → {dests}  {}\n",
                    decode(&ev.packet).summary()
                ));
            }
        }
        s
    }

    pub(crate) fn sync_endpoints(&mut self) {
        self.endpoints = self.backend.endpoints().to_vec();
    }

    pub(crate) fn refresh_devices(&mut self) {
        let open_ins: Vec<String> = self.open_inputs.iter().cloned().collect();
        let open_outs: Vec<String> = self.open_outputs.iter().cloned().collect();
        for id in &open_ins {
            let _ = self.set_input_open(&EndpointId(id.clone()), false);
        }
        for id in &open_outs {
            let _ = self.set_output_open(&EndpointId(id.clone()), false);
        }
        match self.backend.refresh() {
            Ok(()) => {
                self.status.clear();
                self.port_errors.clear();
            }
            Err(err) => self.status = err.to_string(),
        }
        self.backend_name = self.backend.name().to_string();
        self.wms_note = probe_wms().summary;
        self.sync_endpoints();
        let known: HashSet<String> = self.endpoints.iter().map(|e| e.id.0.clone()).collect();
        for id in open_ins {
            if known.contains(&id)
                && let Err(err) = self.set_input_open(&EndpointId(id.clone()), true)
            {
                self.port_errors.insert(id, err);
            }
        }
        for id in open_outs {
            if known.contains(&id)
                && let Err(err) = self.set_output_open(&EndpointId(id.clone()), true)
            {
                self.port_errors.insert(id, err);
            }
        }
        if self.status.is_empty() {
            self.status = format!("{} endpoint(s)", self.endpoints.len());
        }
        self.device_fp = device_fingerprint(&self.endpoints);
    }

    fn tick_throttle(&mut self) {
        let gap = Duration::from_millis(u64::from(self.throttle_ms).max(1));
        let dests: Vec<String> = self.throttle_q.keys().cloned().collect();
        let now = Instant::now();
        for dest in dests {
            if self.throttle_ms == 0 {
                while let Some(packet) = self.throttle_q.get_mut(&dest).and_then(|q| q.pop_front())
                {
                    let _ = self.backend.send(&EndpointId(dest.clone()), &packet);
                }
                continue;
            }
            let ready = self
                .throttle_at
                .get(&dest)
                .is_none_or(|t| now.saturating_duration_since(*t) >= gap);
            if !ready {
                continue;
            }
            let Some(packet) = self.throttle_q.get_mut(&dest).and_then(|q| q.pop_front()) else {
                continue;
            };
            if let Err(err) = self.backend.send(&EndpointId(dest.clone()), &packet) {
                self.port_errors.insert(dest.clone(), err.to_string());
            }
            self.throttle_at.insert(dest, Instant::now());
        }
        self.throttle_q.retain(|_, q| !q.is_empty());
    }

    fn poll_hotplug(&mut self) {
        if self.last_hotplug.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_hotplug = Instant::now();
        let before = self.device_fp.clone();
        if self.backend.refresh().is_err() {
            return;
        }
        self.sync_endpoints();
        let after = device_fingerprint(&self.endpoints);
        if after == before {
            return;
        }
        self.refresh_devices();
        self.status = "MIDI devices changed — rescanned".into();
    }

    pub(crate) fn send_packet(
        &mut self,
        id: &EndpointId,
        packet: &midi_forge_core::UmpMessage,
    ) -> Result<(), String> {
        self.backend.send(id, packet).map_err(|e| e.to_string())
    }

    fn to_profile(&self) -> Profile {
        let links = self
            .router
            .links()
            .iter()
            .filter_map(|l| {
                Some(ProfileLink {
                    from: self.endpoint_by_port.get(&l.from)?.0.clone(),
                    to: self.endpoint_by_port.get(&l.to)?.0.clone(),
                    filter: l.filter.clone(),
                    map: l.map.clone(),
                })
            })
            .collect();
        let mut profile = Profile::new(links);
        profile.lua = self.script.source.clone();
        profile.lua_enabled = self.script.enabled();
        profile.lua_state = self.script.export_state();
        profile.name = self.scene_name.clone();
        profile.mute_clock = self.mute_clock;
        profile.throttle_ms = self.throttle_ms;
        profile.mpe_members = self.mpe_members;
        profile.scenes = self.scenes.clone();
        profile
    }

    fn capture_scene(&self) -> Scene {
        self.to_profile().current_scene()
    }

    pub(crate) fn save_named_scene(&mut self) {
        let mut scene = self.capture_scene();
        scene.name = self.scene_name.trim().to_string();
        if scene.name.is_empty() {
            self.status = "Name the scene first".into();
            return;
        }
        if let Some(existing) = self.scenes.iter_mut().find(|s| s.name == scene.name) {
            *existing = scene;
        } else {
            self.scenes.push(scene);
        }
        self.status = format!("Scene '{}' saved ({})", self.scene_name, self.scenes.len());
    }

    pub(crate) fn recall_scene(&mut self, name: &str) {
        let Some(scene) = self.scenes.iter().find(|s| s.name == name).cloned() else {
            self.status = format!("No scene '{name}'");
            return;
        };
        let mut profile = self.to_profile();
        profile.apply_scene(&scene);
        self.apply_profile(profile);
        self.status = format!("Recalled scene '{name}'");
    }

    fn apply_profile(&mut self, profile: Profile) {
        self.mute_clock = profile.mute_clock;
        self.throttle_ms = profile.throttle_ms.min(50);
        self.mpe_members = if profile.mpe_members == 0 {
            15
        } else {
            profile.mpe_members.min(15)
        };
        self.scene_name = if profile.name.is_empty() {
            "Default".into()
        } else {
            profile.name.clone()
        };
        if !profile.scenes.is_empty() {
            self.scenes = profile.scenes.clone();
        }
        self.router.clear();
        self.selected_link = None;
        let mut loaded = 0usize;
        let mut skipped = 0usize;
        for link in profile.links {
            let from = EndpointId(link.from);
            let to = EndpointId(link.to);
            let known = self.endpoints.iter().any(|e| e.id == from)
                && self.endpoints.iter().any(|e| e.id == to);
            if !known {
                skipped += 1;
                continue;
            }
            match self.set_thru(&from, &to, true) {
                Ok(()) => {
                    let fp = self.ensure_port(&from);
                    let tp = self.ensure_port(&to);
                    self.router.set_filter(fp, tp, link.filter);
                    self.router.set_map(fp, tp, link.map);
                    loaded += 1;
                }
                Err(err) => {
                    self.port_errors.insert(to.0, err);
                    skipped += 1;
                }
            }
        }
        script::apply_profile_lua(self, profile.lua, profile.lua_enabled);
        if !profile.lua_state.is_empty() {
            self.script.import_state(&profile.lua_state);
        }
        self.status = format!("Loaded {loaded} thru links ({skipped} skipped)");
    }

    fn save_profile_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Midi-Forge profile", &["json"])
            .set_file_name("midi-forge.json")
            .save_file()
        else {
            return;
        };
        match self.to_profile().to_json() {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(()) => self.status = format!("Saved {}", path.display()),
                Err(err) => self.status = format!("Save failed: {err}"),
            },
            Err(err) => self.status = format!("Save failed: {err}"),
        }
    }

    fn load_profile_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Midi-Forge profile", &["json"])
            .pick_file()
        else {
            return;
        };
        match std::fs::read_to_string(&path) {
            Ok(json) => match Profile::from_json(&json) {
                Ok(profile) => self.apply_profile(profile),
                Err(err) => self.status = format!("Load failed: {err}"),
            },
            Err(err) => self.status = format!("Load failed: {err}"),
        }
    }
}

impl eframe::App for MidiForgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        {
            let mut eng = self.eng();
            sysex::tick_send(&mut eng, ui.ctx());
        }
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        let mut eng = self.eng();
        EngineInner::ui(&mut eng, ui, frame);
    }
}

impl EngineInner {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("banner").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Midi-Forge");
                ui.separator();
                ui.label("0.1 Beta");
                ui.separator();
                if ui.button("Save").clicked() {
                    self.save_profile_dialog();
                }
                if ui.button("Load").clicked() {
                    self.load_profile_dialog();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.scene_name)
                        .desired_width(90.0)
                        .hint_text("Scene"),
                );
                let scene_names: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
                let mut pick = self.scene_name.clone();
                egui::ComboBox::from_id_salt("scene_pick")
                    .selected_text(if scene_names.is_empty() {
                        "scenes".into()
                    } else {
                        pick.clone()
                    })
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        for n in &scene_names {
                            ui.selectable_value(&mut pick, n.clone(), n);
                        }
                    });
                if pick != self.scene_name && scene_names.iter().any(|n| n == &pick) {
                    self.recall_scene(&pick);
                }
                if ui
                    .small_button("Save scene")
                    .on_hover_text("Store current thru, Lua, mute clock, throttle as a named scene")
                    .clicked()
                {
                    self.save_named_scene();
                }
                ui.separator();
                if ui
                    .selectable_label(self.paused, if self.paused { "Paused" } else { "Pause" })
                    .clicked()
                {
                    self.paused = !self.paused;
                }
                if ui.button("Clear").clicked() {
                    self.log.clear();
                }
                if ui
                    .add_sized(
                        [76.0, 24.0],
                        egui::Button::new(
                            egui::RichText::new("PANIC")
                                .strong()
                                .color(egui::Color32::from_rgb(220, 80, 80)),
                        ),
                    )
                    .on_hover_text("All Sound Off, Reset CC, All Notes Off on every channel")
                    .clicked()
                {
                    self.panic_now();
                }
                if ui.button("Snap").clicked() {
                    let text = self.snapshot_text();
                    ui.ctx().copy_text(text);
                    self.status = "Snapshot copied".into();
                }
                if ui
                    .checkbox(&mut self.always_on_top, "On top")
                    .on_hover_text("Keep Midi-Forge above other windows (live / theatre)")
                    .changed()
                {
                    let level = if self.always_on_top {
                        egui::viewport::WindowLevel::AlwaysOnTop
                    } else {
                        egui::viewport::WindowLevel::Normal
                    };
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
                }
                ui.checkbox(&mut self.mute_clock, "Mute clock")
                    .on_hover_text("Drop MIDI clock and active sensing on thru. Monitor still shows them.");
                ui.checkbox(&mut self.follow, "Follow");
                ui.separator();
                ui.label(format!("{} events", self.log.len()));
                ui.weak(format!("{} thru", self.router.links().len()));
                if self.script.enabled() {
                    ui.colored_label(egui::Color32::from_rgb(80, 180, 140), "Lua");
                }
                if self.dropped > 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 140, 40),
                        format!("{} dropped", self.dropped),
                    );
                }
                if self.log.evicted() > 0 {
                    ui.weak(format!("{} evicted", self.log.evicted()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.backend_name.contains("midisrv") {
                        ui.colored_label(
                            egui::Color32::from_rgb(80, 180, 140),
                            "MidiSrv",
                        )
                        .on_hover_text(
                            "Windows MIDI Services is running. WinMM sees MIDI 1 views of UMP devices. Native MidiSession I/O is a later phase.",
                        );
                    }
                    let caps = self.backend.caps();
                    if caps.native_ump {
                        ui.colored_label(egui::Color32::from_rgb(80, 180, 140), "native UMP")
                            .on_hover_text(
                                "This backend can pass Universal MIDI Packets. WinMM still downscales to MIDI 1.",
                            );
                    } else {
                        ui.weak("MIDI 1 wire")
                            .on_hover_text(
                                "WinMM downscales MIDI 2 to 7-bit. Loopbacks and a future MidiSession keep UMP. See docs/superpowers/specs/2026-08-26-midi2-roadmap.md",
                            );
                    }
                    ui.weak(format!("backend: {}", self.backend_name))
                        .on_hover_text(&self.wms_note);
                });
            });
            if !self.status.is_empty() {
                ui.weak(&self.status);
            }
        });

        egui::Panel::left("ports")
            .default_size(340.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Endpoints");
                    if ui
                        .small_button("Refresh")
                        .on_hover_text("Re-scan MIDI devices")
                        .clicked()
                    {
                        self.refresh_devices();
                    }
                });
                ui.weak("Check an output to open it. Thru cells open both ends.");
                ui.weak(&self.wms_note);
                ui.separator();
                let endpoints = self.endpoints.clone();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for ep in &endpoints {
                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                let mut open = match ep.direction {
                                    Direction::Input => self.open_inputs.contains(&ep.id.0),
                                    Direction::Output | Direction::Bidirectional => {
                                        self.open_outputs.contains(&ep.id.0)
                                    }
                                };
                                if ui.checkbox(&mut open, "").changed() {
                                    let result = match ep.direction {
                                        Direction::Input => self.set_input_open(&ep.id, open),
                                        Direction::Output | Direction::Bidirectional => {
                                            self.set_output_open(&ep.id, open)
                                        }
                                    };
                                    if let Err(err) = result {
                                        self.port_errors.insert(ep.id.0.clone(), err);
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        activity_dot(ui, self.activity.get(&ep.id.0));
                                        ui.strong(&ep.name);
                                        ui.weak(ep.protocol.label());
                                    });
                                    ui.monospace(&ep.id.0);
                                    ui.label(direction_label(ep.direction));
                                });
                            });
                            if let Some(err) = self.port_errors.get(&ep.id.0) {
                                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
                            }
                        });
                    }
                });
                mpe::virtual_cables_ui(ui, self);
            });

        egui::Panel::right("sysex")
            .default_size(360.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.right_tab == RightTab::Sysex, "SysEx")
                        .clicked()
                    {
                        self.right_tab = RightTab::Sysex;
                    }
                    if ui
                        .selectable_label(self.right_tab == RightTab::Lua, "Lua")
                        .clicked()
                    {
                        self.right_tab = RightTab::Lua;
                    }
                    if ui
                        .selectable_label(self.right_tab == RightTab::Net, "Net")
                        .clicked()
                    {
                        self.right_tab = RightTab::Net;
                    }
                });
                ui.separator();
                match self.right_tab {
                    RightTab::Sysex => sysex::librarian_panel(ui, self),
                    RightTab::Lua => script::lua_panel(ui, self),
                    RightTab::Net => net_panel(ui, self),
                }
            });

        egui::Panel::bottom("thru")
            .default_size(320.0)
            .resizable(true)
            .show(ui, |ui| {
                thru::thru_panel(ui, self);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            live::live_panel(ui, self);
            ui.separator();
            clock::clock_panel(ui, self);
            ui.separator();
            clock::route_panel(ui, self);
            ui.separator();
            mpe::mpe_panel(ui, self);
            stuck_notes_panel(ui, self);
            ui.separator();
            inject::inject_panel(ui, self);
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("Monitor");
                if self.paused {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 140, 40),
                        "log frozen — thru still live",
                    );
                }
            });
            monitor_toolbar(ui, self);
            ui.separator();
            header_row(ui);
            let visible = visible_log_indices(self);
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            let n = visible.len();
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.follow)
                .auto_shrink([false, false])
                .show_rows(ui, row_height, n, |ui, range| {
                    for row in range {
                        if let Some(&i) = visible.get(row)
                            && let Some(event) = self.log.get(i)
                        {
                            event_row(ui, event, &self.port_names);
                        }
                    }
                });
        });
    }
}

fn header_row(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.monospace(egui::RichText::new(format!("{:<10}", "Time")).strong());
        ui.monospace(egui::RichText::new(format!("{:<20}", "Port")).strong());
        ui.monospace(egui::RichText::new(format!("{:<14}", "Hex")).strong());
        ui.monospace(egui::RichText::new("Decoded").strong());
    });
}

fn event_row(ui: &mut egui::Ui, event: &MidiEvent, names: &HashMap<PortId, String>) {
    let time = format!("{:>8.3}", event.time.nanos as f64 / 1_000_000_000.0);
    let port = names
        .get(&event.port)
        .cloned()
        .unwrap_or_else(|| format!("port {}", event.port.0));
    let hex = format_wire_hex(&event.packet);
    let decoded = decode(&event.packet).summary();
    ui.horizontal(|ui| {
        ui.monospace(format!("{time:<10}"));
        ui.monospace(format!("{:<20}", truncate(&port, 20)));
        ui.monospace(format!("{hex:<14}"));
        ui.monospace(decoded);
    });
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}

fn direction_label(dir: Direction) -> &'static str {
    match dir {
        Direction::Input => "Input",
        Direction::Output => "Output",
        Direction::Bidirectional => "Bidirectional",
    }
}

fn device_fingerprint(endpoints: &[Endpoint]) -> String {
    let mut parts: Vec<String> = endpoints
        .iter()
        .map(|e| format!("{}|{}", e.id.0, e.name))
        .collect();
    parts.sort();
    parts.join(";")
}

fn activity_dot(ui: &mut egui::Ui, last: Option<&Instant>) {
    let age = last.map(|t| t.elapsed()).unwrap_or(Duration::from_secs(60));
    let color = if age < Duration::from_millis(250) {
        egui::Color32::from_rgb(80, 220, 120)
    } else if age < Duration::from_secs(2) {
        egui::Color32::from_rgb(80, 140, 90)
    } else {
        egui::Color32::from_rgb(50, 50, 55)
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
}

fn net_panel(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.heading("Network MIDI 2.0");
    ui.weak("UDP UMP + invitation commands (M2-124 subset). Auth later.");
    ui.horizontal(|ui| {
        ui.label("Bind");
        ui.add(egui::TextEdit::singleline(&mut app.net.bind).desired_width(140.0));
        if ui.button("Listen").clicked() {
            match app.net.listen() {
                Ok(()) => app.status = app.net.last.clone(),
                Err(err) => app.status = err.to_string(),
            }
        }
        if ui.button("Close").clicked() {
            app.net.close();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Peer");
        ui.add(egui::TextEdit::singleline(&mut app.net.peer).desired_width(140.0));
        if ui.button("Invite").clicked() {
            let pkt = midi_forge_core::invitation("Midi-Forge");
            match app.net.send_bytes(&pkt) {
                Ok(()) => app.status = "invitation sent".into(),
                Err(err) => app.status = err.to_string(),
            }
        }
    });
    ui.weak(&app.net.last);
}

fn stuck_notes_panel(ui: &mut egui::Ui, app: &mut EngineInner) {
    let notes = app.hang.notes();
    if notes.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            egui::Color32::from_rgb(220, 140, 40),
            format!("{} stuck", notes.len()),
        );
        for n in &notes {
            ui.monospace(format!("Ch{} {}", n.channel + 1, n.note));
        }
        if ui.small_button("Off hanging").clicked() {
            let packets = app.hang.note_off_packets();
            let dests: Vec<String> = app.open_outputs.iter().cloned().collect();
            for id in dests {
                for p in &packets {
                    let _ = app.backend.send(&EndpointId(id.clone()), p);
                }
            }
            app.hang.clear();
        }
    });
}

fn monitor_toolbar(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.mon_search)
                .desired_width(140.0)
                .hint_text("Search"),
        );
        ui.checkbox(&mut app.mon_notes, "Notes");
        ui.checkbox(&mut app.mon_cc, "CC");
        ui.checkbox(&mut app.mon_clock, "Clock");
        ui.checkbox(&mut app.mon_sysex, "SysEx");
        ui.checkbox(&mut app.mon_other, "Other");
        ui.label("Ch");
        ui.add(egui::DragValue::new(&mut app.mon_channel).range(0..=16))
            .on_hover_text("0 = all channels");
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(format_visible_log(app));
            app.status = "Copied visible log".into();
        }
        if ui.button("Export").clicked() {
            export_visible_log(app);
        }
        let rec = if app.recorder.recording {
            "Stop rec"
        } else {
            "Record"
        };
        if ui
            .button(rec)
            .on_hover_text("Record the monitor to SMF0")
            .clicked()
        {
            app.recorder.recording = !app.recorder.recording;
            if app.recorder.recording {
                app.recorder.clear();
                app.status = "Recording SMF…".into();
            } else {
                app.status = format!("Recorded {} events", app.recorder.len());
            }
        }
        if !app.recorder.recording && app.recorder.len() > 0 && ui.button("Save SMF").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Standard MIDI", &["mid"])
                .set_file_name("forge.mid")
                .save_file()
            {
                match std::fs::write(&path, app.recorder.to_smf()) {
                    Ok(()) => app.status = format!("Wrote {}", path.display()),
                    Err(err) => app.status = err.to_string(),
                }
            }
        }
        if ui.button("Play SMF").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Standard MIDI", &["mid"])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(bytes) => match midi_forge_core::events_from_smf0(&bytes) {
                        Ok(evs) => {
                            let dest = app.inject_dest.clone();
                            if let Some(id) = dest {
                                let id = EndpointId(id);
                                let _ = app.set_output_open(&id, true);
                                for e in &evs {
                                    let _ = app.send_packet(&id, &e.packet);
                                }
                                app.status = format!("Played {} events", evs.len());
                            } else {
                                app.status = "Pick an inject output first".into();
                            }
                        }
                        Err(err) => app.status = err,
                    },
                    Err(err) => app.status = err.to_string(),
                }
            }
        }
        ui.weak(format!("{} rec", app.recorder.len()));
    });
}

fn event_passes_monitor(app: &EngineInner, event: &MidiEvent) -> bool {
    let kind = message_kind(&event.packet);
    let type_ok = match kind {
        MessageKind::Note | MessageKind::PerNote => app.mon_notes,
        MessageKind::ControlChange => app.mon_cc,
        MessageKind::Clock | MessageKind::ActiveSensing => app.mon_clock,
        MessageKind::Sysex => app.mon_sysex,
        _ => app.mon_other,
    };
    if !type_ok {
        return false;
    }
    if app.mon_channel > 0 {
        match event.packet.channel() {
            Some(ch) if ch + 1 == app.mon_channel => {}
            Some(_) => return false,
            None => {}
        }
    }
    if app.mon_search.is_empty() {
        return true;
    }
    let q = app.mon_search.to_lowercase();
    let hex = format_wire_hex(&event.packet);
    let decoded = decode(&event.packet).summary();
    let port = app.port_names.get(&event.port).cloned().unwrap_or_default();
    hex.to_lowercase().contains(&q)
        || decoded.to_lowercase().contains(&q)
        || port.to_lowercase().contains(&q)
}

fn visible_log_indices(app: &EngineInner) -> Vec<usize> {
    (0..app.log.len())
        .filter(|&i| app.log.get(i).is_some_and(|e| event_passes_monitor(app, e)))
        .collect()
}

fn format_event_line(event: &MidiEvent, names: &HashMap<PortId, String>) -> String {
    let time = event.time.nanos as f64 / 1_000_000_000.0;
    let port = names
        .get(&event.port)
        .cloned()
        .unwrap_or_else(|| format!("port {}", event.port.0));
    format!(
        "{time:.3}\t{port}\t{}\t{}",
        format_wire_hex(&event.packet),
        decode(&event.packet).summary()
    )
}

fn format_visible_log(app: &EngineInner) -> String {
    visible_log_indices(app)
        .into_iter()
        .filter_map(|i| app.log.get(i))
        .map(|e| format_event_line(e, &app.port_names))
        .collect::<Vec<_>>()
        .join("\n")
}

fn export_visible_log(app: &mut EngineInner) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Text", &["txt", "csv"])
        .set_file_name("midi-forge-log.txt")
        .save_file()
    else {
        return;
    };
    match std::fs::write(&path, format_visible_log(app)) {
        Ok(()) => app.status = format!("Exported {}", path.display()),
        Err(err) => app.status = format!("Export failed: {err}"),
    }
}
