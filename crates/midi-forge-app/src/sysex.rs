use std::time::{Duration, Instant};

use eframe::egui;
use midi_forge_core::{
    SysexAssembler, SysexDump, dumps_from_hex, dumps_from_syx, dumps_to_syx, hex_diff,
    parse_identity_reply,
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
        handshake: bool,
        waiting: bool,
        tries: u8,
    },
}

pub enum Wizard {
    Idle,
    Identify {
        dest: String,
        deadline: Instant,
        left: u8,
    },
}

pub struct Librarian {
    pub armed: bool,
    pub assembler: SysexAssembler,
    pub dumps: Vec<SysexDump>,
    pub selected: Option<usize>,
    pub hex_edit: String,
    pub delay_ms: u32,
    pub handshake: bool,
    pub handshake_ms: u32,
    pub got_f7: bool,
    pub dest: Option<String>,
    pub send_job: SendJob,
    pub wizard: Wizard,
    pub identity_note: String,
    pub identity_stem: String,
    pub diff_a: u32,
    pub diff_b: u32,
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
            handshake: false,
            handshake_ms: 2000,
            got_f7: false,
            dest: None,
            send_job: SendJob::Idle,
            wizard: Wizard::Idle,
            identity_note: String::new(),
            identity_stem: String::new(),
            diff_a: 0,
            diff_b: 0,
        }
    }

    pub fn on_packet(&mut self, packet: &midi_forge_core::UmpMessage) {
        if !self.armed {
            return;
        }
        if let Some(dump) = self.assembler.push(packet) {
            if let Some(id) = parse_identity_reply(&dump) {
                self.identity_note = id.summary();
                self.identity_stem = id.file_stem();
            }
            self.hex_edit = dump.to_hex();
            self.dumps.push(dump);
            self.selected = Some(self.dumps.len() - 1);
            self.got_f7 = true;
        }
    }

    pub fn sending(&self) -> bool {
        !matches!(self.send_job, SendJob::Idle)
    }
}

pub fn librarian_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.heading("SysEx");
    ui.weak("Arm receive, then dump from hardware. Handshake waits for an F7 before the next dump.");
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
            app.librarian.identity_stem.clear();
            app.librarian.wizard = Wizard::Idle;
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
        ui.label("Gap after F7");
        ui.add(
            egui::DragValue::new(&mut app.librarian.delay_ms)
                .range(0..=2000)
                .suffix(" ms"),
        );
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.librarian.handshake, "Handshake")
            .on_hover_text("Wait for a received F7 (or timeout) before sending the next dump.");
        ui.label("wait");
        ui.add(
            egui::DragValue::new(&mut app.librarian.handshake_ms)
                .range(200..=10_000)
                .suffix(" ms"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Thru gap");
        ui.add(
            egui::DragValue::new(&mut app.throttle_ms)
                .range(0..=50)
                .suffix(" ms"),
        )
        .on_hover_text("Minimum gap between short MIDI messages on thru (0 = off). Helps vintage UARTs.");
    });
    ui.horizontal(|ui| {
        if ui.button("Identity").clicked() {
            identity_request(app);
        }
        if ui.button("Dump wizard").clicked() {
            start_wizard(app);
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
    match &app.librarian.wizard {
        Wizard::Idle => {}
        Wizard::Identify { left, .. } => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 140, 40),
                format!("Wizard: waiting for identity ({left} tries left)"),
            );
        }
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
                        .selectable_label(selected, format!("#{i} {} B", dump.bytes().len()))
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

    if app.librarian.dumps.len() >= 2 {
        ui.separator();
        ui.label("Hex diff");
        ui.horizontal(|ui| {
            ui.label("A");
            ui.add(
                egui::DragValue::new(&mut app.librarian.diff_a)
                    .range(0..=app.librarian.dumps.len().saturating_sub(1) as u32)
                    .prefix("#"),
            );
            ui.label("B");
            ui.add(
                egui::DragValue::new(&mut app.librarian.diff_b)
                    .range(0..=app.librarian.dumps.len().saturating_sub(1) as u32)
                    .prefix("#"),
            );
        });
        let max = app.librarian.dumps.len().saturating_sub(1) as u32;
        let a = app.librarian.diff_a.min(max) as usize;
        let b = app.librarian.diff_b.min(max) as usize;
        let text = hex_diff(
            app.librarian.dumps[a].bytes(),
            app.librarian.dumps[b].bytes(),
        );
        egui::ScrollArea::vertical()
            .max_height(80.0)
            .id_salt("sysex_diff")
            .show(ui, |ui| {
                ui.monospace(text);
            });
    }
}

pub fn tick_send(app: &mut MidiForgeApp, ctx: &egui::Context) {
    tick_wizard(app, ctx);
    let job = std::mem::replace(&mut app.librarian.send_job, SendJob::Idle);
    match job {
        SendJob::Idle => {}
        SendJob::Active {
            dest,
            dumps,
            index,
            next_at,
            handshake,
            waiting,
            tries,
        } => {
            if waiting {
                if app.librarian.got_f7 {
                    app.librarian.got_f7 = false;
                    advance_send(app, ctx, dest, dumps, index + 1, handshake);
                    return;
                }
                if Instant::now() < next_at {
                    ctx.request_repaint_after(next_at.saturating_duration_since(Instant::now()));
                    app.librarian.send_job = SendJob::Active {
                        dest,
                        dumps,
                        index,
                        next_at,
                        handshake,
                        waiting: true,
                        tries,
                    };
                    return;
                }
                if tries > 0 {
                    app.status = format!("Handshake timeout, retrying dump {}", index + 1);
                    send_one(app, ctx, dest, dumps, index, handshake, tries - 1);
                } else {
                    app.status = format!("Handshake timeout on dump {}", index + 1);
                }
                return;
            }
            if Instant::now() < next_at {
                ctx.request_repaint_after(next_at.saturating_duration_since(Instant::now()));
                app.librarian.send_job = SendJob::Active {
                    dest,
                    dumps,
                    index,
                    next_at,
                    handshake,
                    waiting: false,
                    tries,
                };
                return;
            }
            if index >= dumps.len() {
                app.status = format!("Sent {} SysEx dump(s)", dumps.len());
                return;
            }
            send_one(app, ctx, dest, dumps, index, handshake, 2);
        }
    }
}

fn send_one(
    app: &mut MidiForgeApp,
    ctx: &egui::Context,
    dest: String,
    dumps: Vec<Vec<u8>>,
    index: usize,
    handshake: bool,
    tries: u8,
) {
    let id = EndpointId(dest.clone());
    match app.send_sysex_now(&id, &dumps[index]) {
        Ok(()) => {
            let next = index + 1;
            if next >= dumps.len() {
                app.status = format!("Sent {} SysEx dump(s)", dumps.len());
                return;
            }
            if handshake {
                app.librarian.got_f7 = false;
                let wait = Duration::from_millis(u64::from(app.librarian.handshake_ms).max(1));
                app.librarian.send_job = SendJob::Active {
                    dest,
                    dumps,
                    index,
                    next_at: Instant::now() + wait,
                    handshake,
                    waiting: true,
                    tries,
                };
                ctx.request_repaint_after(Duration::from_millis(50));
            } else {
                advance_send(app, ctx, dest, dumps, next, handshake);
            }
        }
        Err(err) => {
            app.status = format!("SysEx send failed: {err}");
            app.port_errors.insert(dest, err);
        }
    }
}

fn advance_send(
    app: &mut MidiForgeApp,
    ctx: &egui::Context,
    dest: String,
    dumps: Vec<Vec<u8>>,
    next: usize,
    handshake: bool,
) {
    if next >= dumps.len() {
        app.status = format!("Sent {} SysEx dump(s)", dumps.len());
        return;
    }
    let wait = Duration::from_millis(u64::from(app.librarian.delay_ms));
    app.librarian.send_job = SendJob::Active {
        dest,
        dumps,
        index: next,
        next_at: Instant::now() + wait,
        handshake,
        waiting: false,
        tries: 2,
    };
    ctx.request_repaint_after(wait.max(Duration::from_millis(16)));
}

fn tick_wizard(app: &mut MidiForgeApp, ctx: &egui::Context) {
    let Wizard::Identify {
        dest,
        deadline,
        left,
    } = std::mem::replace(&mut app.librarian.wizard, Wizard::Idle)
    else {
        return;
    };
    if !app.librarian.identity_note.is_empty() {
        app.status = format!("Wizard: {}", app.librarian.identity_note);
        return;
    }
    if Instant::now() < deadline {
        app.librarian.wizard = Wizard::Identify {
            dest,
            deadline,
            left,
        };
        ctx.request_repaint_after(Duration::from_millis(100));
        return;
    }
    if left == 0 {
        app.status = "Wizard: no identity reply".into();
        return;
    }
    let id = EndpointId(dest.clone());
    match app.send_sysex_now(&id, &midi_forge_core::IDENTITY_REQUEST) {
        Ok(()) => {
            app.status = format!("Wizard: identity retry ({left} left)");
            app.librarian.wizard = Wizard::Identify {
                dest,
                deadline: Instant::now() + Duration::from_secs(2),
                left: left - 1,
            };
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        Err(err) => app.status = format!("Wizard failed: {err}"),
    }
}

fn start_wizard(app: &mut MidiForgeApp) {
    let Some(dest) = app.librarian.dest.clone() else {
        app.status = "Pick a SysEx output".into();
        return;
    };
    let id = EndpointId(dest.clone());
    if let Err(err) = app.set_output_open(&id, true) {
        app.port_errors.insert(id.0, err);
        return;
    }
    app.librarian.armed = true;
    app.librarian.identity_note.clear();
    app.librarian.identity_stem.clear();
    match app.send_sysex_now(&id, &midi_forge_core::IDENTITY_REQUEST) {
        Ok(()) => {
            app.status = "Wizard: identity sent, waiting…".into();
            app.librarian.wizard = Wizard::Identify {
                dest,
                deadline: Instant::now() + Duration::from_secs(2),
                left: 2,
            };
        }
        Err(err) => app.status = format!("Wizard failed: {err}"),
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
    app.librarian.got_f7 = false;
    app.librarian.send_job = SendJob::Active {
        dest,
        dumps: dumps.into_iter().map(|d| d.bytes().to_vec()).collect(),
        index: 0,
        next_at: Instant::now(),
        handshake: app.librarian.handshake,
        waiting: false,
        tries: 2,
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
        .set_file_name(if app.librarian.identity_stem.is_empty() {
            "dump.syx".into()
        } else {
            format!("{}.syx", app.librarian.identity_stem)
        })
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
