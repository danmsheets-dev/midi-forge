use std::collections::{HashMap, HashSet};
use std::time::Duration;

use eframe::egui;
use midi_forge_core::{
    MidiEvent, MonitorLog, MpeTracker, PortId, Profile, ProfileLink, Router, SysexAssembler,
    decode, format_wire_hex, panic_packets,
};
use midi_forge_io::{Direction, Endpoint, EndpointId, MidiBackend, default_backend};

use crate::mpe;
use crate::script::{self, RightTab};
use crate::sysex::{self, Librarian};
use crate::thru;

pub struct MidiForgeApp {
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
}

impl MidiForgeApp {
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
        app
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
                .map_err(|e| e.to_string())?;
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
                .map_err(|e| e.to_string())?;
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

    fn drain_capture(&mut self) {
        self.capture_buf.clear();
        self.dropped = self.backend.poll(&mut self.capture_buf);
        let events: Vec<MidiEvent> = self.capture_buf.drain(..).collect();
        if !self.paused {
            for event in &events {
                self.log.push(*event);
            }
        }
        for event in &events {
            self.mpe.push(&event.packet);
            self.librarian.on_packet(&event.packet);
            let processed = self.script.process(event);
            for event in &processed {
                for routed in self.router.route(event) {
                    let Some(dest) = self.endpoint_by_port.get(&routed.port).cloned() else {
                        continue;
                    };
                    if !self.open_outputs.contains(&dest.0) {
                        continue;
                    }
                    if routed.packet.message_type() == 0x3 {
                        let asm = self.thru_sysex.entry(dest.0.clone()).or_default();
                        if let Some(dump) = asm.push(&routed.packet)
                            && let Err(err) = self.backend.send_sysex(&dest, dump.bytes())
                        {
                            self.port_errors.insert(dest.0, err.to_string());
                        }
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
        for id in ids {
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
        self.mpe.clear_voices();
        self.status = format!("Panic: sent {sent} short messages to open outputs");
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
        profile
    }

    fn apply_profile(&mut self, profile: Profile) {
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_capture();
        sysex::tick_send(self, ui.ctx());
        if !self.open_inputs.is_empty() || self.librarian.sending() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

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
                    .button(
                        egui::RichText::new("Panic").color(egui::Color32::from_rgb(220, 80, 80)),
                    )
                    .on_hover_text("All Sound Off, Reset CC, All Notes Off on every channel")
                    .clicked()
                {
                    self.panic_now();
                }
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
                    ui.weak(format!("backend: {}", self.backend_name));
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
                });
                ui.separator();
                match self.right_tab {
                    RightTab::Sysex => sysex::librarian_panel(ui, self),
                    RightTab::Lua => script::lua_panel(ui, self),
                }
            });

        egui::Panel::bottom("thru")
            .default_size(320.0)
            .resizable(true)
            .show(ui, |ui| {
                thru::thru_panel(ui, self);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            mpe::mpe_panel(ui, self);
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
            ui.separator();
            header_row(ui);
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            let n = self.log.len();
            egui::ScrollArea::vertical()
                .stick_to_bottom(self.follow)
                .auto_shrink([false, false])
                .show_rows(ui, row_height, n, |ui, range| {
                    for i in range {
                        if let Some(event) = self.log.get(i) {
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
