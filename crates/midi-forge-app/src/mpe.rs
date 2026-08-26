use eframe::egui;
use midi_forge_core::{MpeZoneKind, bend_semitones, mcm_packets};
use midi_forge_io::{Direction, EndpointId};

use crate::app::MidiForgeApp;

pub fn mpe_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.horizontal(|ui| {
        ui.heading("MPE");
        if app.mpe.configured() {
            ui.weak("zones from RPN 6");
        } else {
            ui.weak("no MCM yet — voices still tracked");
        }
    });
    ui.horizontal(|ui| {
        if let Some(z) = app.mpe.lower_zone() {
            ui.label(format!(
                "Lower: master Ch{} + {} members",
                z.master + 1,
                z.members
            ));
        } else {
            ui.weak("Lower off");
        }
        ui.separator();
        if let Some(z) = app.mpe.upper_zone() {
            ui.label(format!(
                "Upper: master Ch{} + {} members",
                z.master + 1,
                z.members
            ));
        } else {
            ui.weak("Upper off");
        }
        ui.separator();
        ui.weak(format!(
            "PB ±{} / master ±{}",
            app.mpe.note_pitch_bend_range(),
            app.mpe.master_pitch_bend_range()
        ));
    });
    ui.horizontal(|ui| {
        ui.label("Send MCM members");
        ui.add(egui::DragValue::new(&mut app.mpe_members).range(0..=15));
        if ui.button("Lower zone").clicked() {
            send_mcm(app, MpeZoneKind::Lower);
        }
        if ui.button("Upper zone").clicked() {
            send_mcm(app, MpeZoneKind::Upper);
        }
        if ui.button("Clear voices").clicked() {
            app.mpe.clear_voices();
        }
    });

    egui::ScrollArea::vertical()
        .max_height(120.0)
        .id_salt("mpe_voices")
        .show(ui, |ui| {
            if app.mpe.voices().is_empty() {
                ui.weak("No sounding MPE notes.");
                return;
            }
            ui.horizontal(|ui| {
                ui.monospace(egui::RichText::new("Ch").strong());
                ui.monospace(egui::RichText::new("Note").strong());
                ui.monospace(egui::RichText::new("Vel").strong());
                ui.monospace(egui::RichText::new("Bend").strong());
                ui.monospace(egui::RichText::new("Press").strong());
                ui.monospace(egui::RichText::new("Timbre").strong());
                ui.monospace(egui::RichText::new("Role").strong());
            });
            for v in app.mpe.voices() {
                let range = if app.mpe.role(v.channel).contains("master") {
                    app.mpe.master_pitch_bend_range()
                } else {
                    app.mpe.note_pitch_bend_range()
                };
                let bend = bend_semitones(v.pitch_bend, range);
                ui.horizontal(|ui| {
                    ui.monospace(format!("{:>2}", v.channel + 1));
                    ui.monospace(format!("{:>4}", v.note));
                    ui.monospace(format!("{:>3}", v.velocity));
                    ui.monospace(format!("{bend:>+6.2}"));
                    ui.monospace(format!("{:>3}", v.pressure));
                    ui.monospace(format!("{:>3}", v.timbre));
                    ui.weak(app.mpe.role(v.channel));
                });
            }
        });
}

fn send_mcm(app: &mut MidiForgeApp, zone: MpeZoneKind) {
    let Some(dest) = app
        .endpoints
        .iter()
        .find(|e| e.direction == Direction::Output && app.open_outputs.contains(&e.id.0))
        .map(|e| e.id.clone())
        .or_else(|| {
            app.endpoints
                .iter()
                .find(|e| e.direction == Direction::Output)
                .map(|e| e.id.clone())
        })
    else {
        app.status = "Open an output to send MCM".into();
        return;
    };
    if let Err(err) = app.set_output_open(&dest, true) {
        app.port_errors.insert(dest.0.clone(), err);
        return;
    }
    let members = app.mpe_members;
    let mut sent = 0usize;
    for packet in mcm_packets(zone, members) {
        match app.send_packet(&dest, &packet) {
            Ok(()) => sent += 1,
            Err(err) => {
                app.status = format!("MCM send failed: {err}");
                return;
            }
        }
    }
    app.status = format!(
        "Sent MCM {zone:?} members={members} ({sent} packets) to {}",
        dest.0
    );
}

pub fn virtual_cables_ui(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.separator();
    ui.heading("Virtual cables");
    ui.weak("App-local loopbacks. DAWs see loopMIDI / MIDI Services ports already listed above.");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.cable_name)
                .desired_width(140.0)
                .hint_text("Forge Cable"),
        );
        if ui.button("Add cable").clicked() {
            add_cable(app);
        }
    });
    let forge: Vec<EndpointId> = app
        .endpoints
        .iter()
        .filter(|e| e.id.0.starts_with("forge:loop:") && e.direction == Direction::Input)
        .map(|e| e.id.clone())
        .collect();
    for id in forge {
        ui.horizontal(|ui| {
            let name = app
                .endpoints
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| id.0.clone());
            ui.label(name);
            if ui.small_button("Remove").clicked() {
                remove_cable(app, &id);
            }
        });
    }
}

fn add_cable(app: &mut MidiForgeApp) {
    let name = app.cable_name.clone();
    match app.backend.create_loopback(&name) {
        Ok((inp, outp)) => {
            app.sync_endpoints();
            let _ = app.set_input_open(&inp, true);
            let _ = app.set_output_open(&outp, true);
            app.status = format!("Cable {} → {}", inp.0, outp.0);
        }
        Err(err) => app.status = format!("Cable failed: {err}"),
    }
}

fn remove_cable(app: &mut MidiForgeApp, id: &EndpointId) {
    let _ = app.set_input_open(id, false);
    if let Some(out) = id.0.strip_suffix(":in") {
        let out_id = EndpointId(format!("{out}:out"));
        let _ = app.set_output_open(&out_id, false);
    }
    match app.backend.remove_loopback(id) {
        Ok(()) => {
            app.sync_endpoints();
            app.status = "Removed Forge cable".into();
        }
        Err(err) => app.status = format!("Remove failed: {err}"),
    }
}
