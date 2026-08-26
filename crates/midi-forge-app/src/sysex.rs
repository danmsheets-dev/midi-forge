use std::time::{Duration, Instant};

use eframe::egui;
use midi_forge_core::{
    SysexAssembler, SysexDump, dumps_from_hex, dumps_from_syx, dumps_to_syx, parse_identity_reply,
};
use midi_forge_io::{Direction, EndpointId};

use crate::app::MidiForgeApp;

pub enum SendJob {
    Idle,
    Active {
        dest: String,
        dumps: Vec<Vec<u8>>,
        index: usize,
        next_at: Instant,
    },
}

pub struct Librarian {
    pub armed: bool,
    pub assembler: SysexAssembler,
    pub dumps: Vec<SysexDump>,
    pub selected: Option<usize>,
    pub hex_edit: String,
    pub delay_ms: u32,
    pub dest: Option<String>,
    pub send_job: SendJob,
    pub identity_note: String,
}

impl Librarian {
    pub fn new() -> Self {
        Self {
            armed: false,
            assembler: SysexAssembler::new(),
            dumps: Vec::new(),
            selected: None,
            hex_edit: SysexDump::identity_request().to_hex(),
            delay_ms: 60,
            dest: None,
            send_job: SendJob::Idle,
            identity_note: String::new(),
        }
    }

    pub fn on_packet(&mut self, packet: &midi_forge_core::UmpMessage) {
        if !self.armed {
            return;
        }
        if let Some(dump) = self.assembler.push(packet) {
            if let Some(id) = parse_identity_reply(&dump) {
                self.identity_note = id.summary();
            }
            self.hex_edit = dump.to_hex();
            self.dumps.push(dump);
            self.selected = Some(self.dumps.len() - 1);
        }
    }

    pub fn sending(&self) -> bool {
        !matches!(self.send_job, SendJob::Idle)
    }
}

pub fn librarian_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.heading("SysEx");
    ui.weak("Arm receive, then dump from hardware. Delay applies after each F7.");
    ui.separator();

    let outputs: Vec<(String, String)> = app
        .endpoints
        .iter()
        .filter(|e| e.direction == Direction::Output)
        .map(|e| (e.id.0.clone(), e.name.clone()))
        .collect();
    if app.librarian.dest.is_none() {
        app.librarian.dest = outputs.first().map(|(id, _)| id.clone());
    }

    ui.horizontal(|ui| {
        ui.checkbox(&mut app.librarian.armed, "Arm receive");
        if ui.button("Clear dumps").clicked() {
            app.librarian.dumps.clear();
            app.librarian.selected = None;
            app.librarian.assembler.reset();
            app.librarian.identity_note.clear();
        }
    });
    ui.horizontal(|ui| {
        ui.label("Output");
        let current = app
            .librarian
            .dest
            .as_ref()
            .and_then(|id| outputs.iter().find(|(oid, _)| oid == id))
            .map(|(_, name)| name.as_str())
            .unwrap_or("(none)");
        egui::ComboBox::from_id_salt("sysex_dest")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in &outputs {
                    ui.selectable_value(&mut app.librarian.dest, Some(id.clone()), name);
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Delay after F7");
        ui.add(
            egui::DragValue::new(&mut app.librarian.delay_ms)
                .range(0..=1000)
                .suffix(" ms"),
        );
    });
    ui.horizontal(|ui| {
        if ui.button("Identity request").clicked() {
            identity_request(app);
        }
        if ui.button("Load .syx").clicked() {
            load_syx(app);
        }
        if ui.button("Save .syx").clicked() {
            save_syx(app);
        }
    });
    if !app.librarian.identity_note.is_empty() {
        ui.weak(&app.librarian.identity_note);
    }
    if app.librarian.sending() {
        ui.colored_label(egui::Color32::from_rgb(220, 140, 40), "Sending…");
    }

    ui.separator();
    ui.label("Captured dumps");
    egui::ScrollArea::vertical()
        .max_height(140.0)
        .id_salt("sysex_dumps")
        .show(ui, |ui| {
            let mut pick = None;
            let mut drop = None;
            for (i, dump) in app.librarian.dumps.iter().enumerate() {
                let selected = app.librarian.selected == Some(i);
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(selected, format!("{} B", dump.bytes().len()))
                        .clicked()
                    {
                        pick = Some(i);
                    }
                    ui.weak(preview_hex(dump.bytes()));
                    if ui.small_button("x").clicked() {
                        drop = Some(i);
                    }
                });
            }
            if let Some(i) = pick {
                app.librarian.selected = Some(i);
                app.librarian.hex_edit = app.librarian.dumps[i].to_hex();
            }
            if let Some(i) = drop {
                app.librarian.dumps.remove(i);
                app.librarian.selected = None;
            }
        });

    ui.separator();
    ui.label("Command (hex)");
    ui.add(
        egui::TextEdit::multiline(&mut app.librarian.hex_edit)
            .desired_rows(6)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
    ui.horizontal_wrapped(|ui| {
        if ui.button("Roland checksum").clicked()
            && let Ok(dumps) = dumps_from_hex(&app.librarian.hex_edit)
            && let Some(dump) = dumps.first()
            && let Ok(fixed) = dump.with_roland_checksum()
        {
            app.librarian.hex_edit = fixed.to_hex();
        }
        if ui.button("Send editor").clicked() {
            match dumps_from_hex(&app.librarian.hex_edit) {
                Ok(dumps) => queue_send(app, dumps),
                Err(err) => app.status = format!("Hex: {err}"),
            }
        }
        if ui.button("Send selected").clicked() {
            if let Some(i) = app.librarian.selected {
                let dump = app.librarian.dumps[i].clone();
                queue_send(app, vec![dump]);
            } else {
                app.status = "No dump selected".into();
            }
        }
        if ui.button("Send all").clicked() {
            queue_send(app, app.librarian.dumps.clone());
        }
    });
}

pub fn tick_send(app: &mut MidiForgeApp, ctx: &egui::Context) {
    let job = std::mem::replace(&mut app.librarian.send_job, SendJob::Idle);
    match job {
        SendJob::Idle => {}
        SendJob::Active {
            dest,
            dumps,
            index,
            next_at,
        } => {
            if Instant::now() < next_at {
                ctx.request_repaint_after(next_at.saturating_duration_since(Instant::now()));
                app.librarian.send_job = SendJob::Active {
                    dest,
                    dumps,
                    index,
                    next_at,
                };
                return;
            }
            if index >= dumps.len() {
                app.status = format!("Sent {} SysEx dump(s)", dumps.len());
                return;
            }
            let id = EndpointId(dest.clone());
            match app.send_sysex_now(&id, &dumps[index]) {
                Ok(()) => {
                    let next = index + 1;
                    if next >= dumps.len() {
                        app.status = format!("Sent {} SysEx dump(s)", dumps.len());
                    } else {
                        let wait = Duration::from_millis(u64::from(app.librarian.delay_ms));
                        app.librarian.send_job = SendJob::Active {
                            dest,
                            dumps,
                            index: next,
                            next_at: Instant::now() + wait,
                        };
                        ctx.request_repaint_after(wait);
                    }
                }
                Err(err) => {
                    app.status = format!("SysEx send failed: {err}");
                    app.port_errors.insert(dest, err);
                }
            }
        }
    }
}

fn identity_request(app: &mut MidiForgeApp) {
    let Some(dest) = app.librarian.dest.clone() else {
        app.status = "Pick a SysEx output".into();
        return;
    };
    let id = EndpointId(dest);
    if let Err(err) = app.set_output_open(&id, true) {
        app.port_errors.insert(id.0.clone(), err);
        return;
    }
    app.librarian.armed = true;
    match app.send_sysex_now(&id, &midi_forge_core::IDENTITY_REQUEST) {
        Ok(()) => app.status = "Identity request sent".into(),
        Err(err) => app.status = format!("Identity failed: {err}"),
    }
}

fn queue_send(app: &mut MidiForgeApp, dumps: Vec<SysexDump>) {
    if dumps.is_empty() {
        app.status = "Nothing to send".into();
        return;
    }
    let Some(dest) = app.librarian.dest.clone() else {
        app.status = "Pick a SysEx output".into();
        return;
    };
    let id = EndpointId(dest.clone());
    if let Err(err) = app.set_output_open(&id, true) {
        app.port_errors.insert(id.0, err);
        return;
    }
    app.librarian.send_job = SendJob::Active {
        dest,
        dumps: dumps.into_iter().map(|d| d.bytes().to_vec()).collect(),
        index: 0,
        next_at: Instant::now(),
    };
}

fn load_syx(app: &mut MidiForgeApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("SysEx", &["syx", "SYX"])
        .add_filter("Hex", &["txt", "hex"])
        .pick_file()
    else {
        return;
    };
    match std::fs::read(&path) {
        Ok(raw) => {
            let parsed = if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("syx"))
            {
                dumps_from_syx(&raw)
            } else {
                dumps_from_hex(&String::from_utf8_lossy(&raw))
            };
            match parsed {
                Ok(dumps) => {
                    if let Some(first) = dumps.first() {
                        app.librarian.hex_edit = first.to_hex();
                    }
                    app.librarian.dumps.extend(dumps);
                    app.status = format!("Loaded {}", path.display());
                }
                Err(err) => app.status = format!("Load failed: {err}"),
            }
        }
        Err(err) => app.status = format!("Load failed: {err}"),
    }
}

fn save_syx(app: &mut MidiForgeApp) {
    let dumps = if let Some(i) = app.librarian.selected {
        vec![app.librarian.dumps[i].clone()]
    } else {
        app.librarian.dumps.clone()
    };
    if dumps.is_empty() {
        app.status = "No dumps to save".into();
        return;
    }
    let Some(path) = rfd::FileDialog::new()
        .add_filter("SysEx", &["syx"])
        .set_file_name("dump.syx")
        .save_file()
    else {
        return;
    };
    match std::fs::write(&path, dumps_to_syx(&dumps)) {
        Ok(()) => app.status = format!("Saved {}", path.display()),
        Err(err) => app.status = format!("Save failed: {err}"),
    }
}

fn preview_hex(bytes: &[u8]) -> String {
    let n = bytes.len().min(12);
    let mut s: String = bytes[..n]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > n {
        s.push('…');
    }
    s
}
