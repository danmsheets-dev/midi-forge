use std::collections::{HashMap, HashSet};
use std::time::Duration;

use eframe::egui;
use midi_forge_core::{MidiEvent, MonitorLog, PortId, decode, format_wire_hex, panic_packets};
use midi_forge_io::{Direction, Endpoint, EndpointId, MidiBackend, default_backend};

pub struct MidiForgeApp {
    backend: Box<dyn MidiBackend>,
    backend_name: String,
    endpoints: Vec<Endpoint>,
    log: MonitorLog,
    paused: bool,
    follow: bool,
    dropped: u64,
    open_inputs: HashSet<String>,
    open_outputs: HashSet<String>,
    port_names: HashMap<PortId, String>,
    port_errors: HashMap<String, String>,
    next_port: u32,
    capture_buf: Vec<MidiEvent>,
    status: String,
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
            paused: false,
            follow: true,
            dropped: 0,
            open_inputs: HashSet::new(),
            open_outputs: HashSet::new(),
            port_names: HashMap::new(),
            port_errors: HashMap::new(),
            next_port: 1,
            capture_buf: Vec::new(),
            status,
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

    fn set_input_open(&mut self, id: &EndpointId, open: bool) -> Result<(), String> {
        if open {
            if self.open_inputs.contains(&id.0) {
                return Ok(());
            }
            let port = PortId(self.next_port);
            self.next_port += 1;
            self.backend
                .open_input(id, port)
                .map_err(|e| e.to_string())?;
            let name = self
                .endpoints
                .iter()
                .find(|e| e.id == *id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| id.0.clone());
            self.port_names.insert(port, name);
            self.open_inputs.insert(id.0.clone());
            self.port_errors.remove(&id.0);
        } else if self.open_inputs.remove(&id.0) {
            self.backend.close_input(id).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn set_output_open(&mut self, id: &EndpointId, open: bool) -> Result<(), String> {
        if open {
            if self.open_outputs.contains(&id.0) {
                return Ok(());
            }
            let port = PortId(self.next_port);
            self.next_port += 1;
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

    fn drain_capture(&mut self) {
        self.capture_buf.clear();
        self.dropped = self.backend.poll(&mut self.capture_buf);
        if self.paused {
            self.capture_buf.clear();
            return;
        }
        for event in self.capture_buf.drain(..) {
            self.log.push(event);
        }
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
        self.status = format!("Panic: sent {sent} short messages to open outputs");
    }
}

impl eframe::App for MidiForgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_capture();
        if !self.open_inputs.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        egui::Panel::top("banner").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Midi-Forge");
                ui.separator();
                ui.label("Phase 1 — live monitor");
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
                ui.heading("Endpoints");
                ui.weak("Inputs auto-open. Check an output to send Panic there.");
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
                                    ui.strong(&ep.name);
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
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Monitor");
                if self.paused {
                    ui.colored_label(egui::Color32::from_rgb(220, 140, 40), "frozen");
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

fn truncate(s: &str, max: usize) -> String {
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
