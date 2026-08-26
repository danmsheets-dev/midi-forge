use eframe::egui;
use midi_forge_io::{Direction, Endpoint, MidiBackend, default_backend};

pub struct MidiForgeApp {
    backend_name: String,
    endpoints: Vec<Endpoint>,
    error: Option<String>,
}

impl MidiForgeApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut backend: Box<dyn MidiBackend> = default_backend();
        let backend_name = backend.name().to_string();
        match backend.refresh() {
            Ok(()) => Self {
                backend_name,
                endpoints: backend.endpoints().to_vec(),
                error: None,
            },
            Err(err) => Self {
                backend_name,
                endpoints: Vec::new(),
                error: Some(err.to_string()),
            },
        }
    }
}

impl eframe::App for MidiForgeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("banner").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Midi-Forge");
                ui.separator();
                ui.label("Phase 0 — workspace + WinMM enumerate");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("backend: {}", self.backend_name));
                });
            });
        });

        egui::Panel::left("ports")
            .default_size(320.0)
            .show(ui, |ui| {
                ui.heading("Endpoints");
                ui.weak("Live input opens in Phase 1.");
                ui.separator();
                if let Some(err) = &self.error {
                    ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
                }
                if self.endpoints.is_empty() && self.error.is_none() {
                    ui.label("No MIDI endpoints. Plug in a device and restart.");
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for ep in &self.endpoints {
                        ui.group(|ui| {
                            ui.strong(&ep.name);
                            ui.monospace(&ep.id.0);
                            ui.label(direction_label(ep.direction));
                        });
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Monitor");
            ui.label(
                "UMP-canonical engine is in midi-forge-core. Streaming a keyboard \
                 into this pane is Phase 1.",
            );
            ui.add_space(12.0);
            ui.label("Run from a terminal to dump ports without the GUI:");
            ui.code("midi-forge --list");
        });
    }
}

fn direction_label(dir: Direction) -> &'static str {
    match dir {
        Direction::Input => "Input",
        Direction::Output => "Output",
        Direction::Bidirectional => "Bidirectional",
    }
}
