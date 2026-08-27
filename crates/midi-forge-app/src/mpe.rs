use eframe::egui;
use midi_forge_core::{MpeZoneKind, bend_semitones, mcm_packets, pitch_bend_range_packets};
use midi_forge_io::{Direction, EndpointId, create_wms_loopback};

use crate::app::EngineInner;

pub fn mpe_panel(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.horizontal(|ui| {
        ui.heading("MPE");
        let summary = app.mpe.mode_summary();
        if app.mpe.configured() {
            ui.colored_label(egui::Color32::from_rgb(80, 200, 120), summary);
        } else if app.mpe.likely_mpe() {
            ui.colored_label(egui::Color32::from_rgb(220, 160, 60), summary);
        } else {
            ui.weak(summary);
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
        if ui
            .button("Note PB 48")
            .on_hover_text("RPN 0 pitch bend range 48 on member-style ch 2")
            .clicked()
        {
            send_pb_range(app, 1, 48);
        }
        if ui
            .button("Master PB 2")
            .on_hover_text("RPN 0 pitch bend range 2 on lower master (ch 1)")
            .clicked()
        {
            send_pb_range(app, 0, 2);
        }
    });

    let voice_h = 120.0;
    ui.allocate_ui(egui::vec2(ui.available_width(), voice_h), |ui| {
        egui::ScrollArea::vertical()
            .max_height(voice_h)
            .min_scrolled_height(voice_h)
            .auto_shrink([false, false])
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
                    mini_bar(ui, v.pressure, egui::Color32::from_rgb(90, 180, 255));
                    mini_bar(ui, v.timbre, egui::Color32::from_rgb(200, 140, 80));
                });
            }
            });
    });
}

fn mini_bar(ui: &mut egui::Ui, value: u8, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(28.0, 10.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 1.0, egui::Color32::from_rgb(40, 40, 48));
    let mut fill = rect;
    fill.set_width(rect.width() * f32::from(value) / 127.0);
    ui.painter().rect_filled(fill, 1.0, color);
}

fn send_packets(app: &mut EngineInner, packets: &[midi_forge_core::UmpMessage], what: &str) {
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
        app.status = format!("Open an output to send {what}");
        return;
    };
    if let Err(err) = app.set_output_open(&dest, true) {
        app.port_errors.insert(dest.0.clone(), err);
        return;
    }
    let mut sent = 0usize;
    for packet in packets {
        match app.send_packet(&dest, packet) {
            Ok(()) => sent += 1,
            Err(err) => {
                app.status = format!("{what} send failed: {err}");
                return;
            }
        }
    }
    app.status = format!("Sent {what} ({sent} packets) to {}", dest.0);
}

fn send_pb_range(app: &mut EngineInner, channel: u8, semitones: u8) {
    send_packets(
        app,
        &pitch_bend_range_packets(channel, semitones),
        &format!("PB range ±{semitones} ch{}", channel + 1),
    );
}

fn send_mcm(app: &mut EngineInner, zone: MpeZoneKind) {
    let members = app.mpe_members;
    send_packets(
        app,
        &mcm_packets(zone, members),
        &format!("MCM {zone:?} members={members}"),
    );
}

fn add_wms_loop(app: &mut EngineInner) {
    match create_wms_loopback(&app.cable_name) {
        Ok(msg) => {
            app.refresh_devices();
            app.status = msg;
        }
        Err(err) => app.status = err,
    }
}

pub fn virtual_cables_ui(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.separator();
    ui.heading("Virtual cables");
    ui.weak(
        "Add cable = in-app (DAWs cannot see it). Add DAW loop = midi.exe WMS pair (DAWs can).",
    );
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.cable_name)
                .desired_width(140.0)
                .hint_text("Forge Cable"),
        );
        if ui.button("Add cable").clicked() {
            add_cable(app);
        }
        if ui
            .button("Add DAW loop")
            .on_hover_text("midi loopback create --root-name (needs MidiSrv + SDK Tools)")
            .clicked()
        {
            add_wms_loop(app);
        }
    });
    let forge: Vec<EndpointId> = app
        .endpoints
        .iter()
        .filter(|e| {
            e.direction == Direction::Input
                && (e.id.0.starts_with("forge:loop:") || e.id.0.starts_with("coremidi:vd:"))
        })
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

fn add_cable(app: &mut EngineInner) {
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

fn remove_cable(app: &mut EngineInner, id: &EndpointId) {
    let _ = app.set_input_open(id, false);
    if let Some(out) = id.0.strip_suffix(":in") {
        let out_id = EndpointId(format!("{out}:out"));
        let _ = app.set_output_open(&out_id, false);
    } else if let Some(idx) = id.0.strip_prefix("coremidi:vd:") {
        let _ = app.set_output_open(&EndpointId(format!("coremidi:vs:{idx}")), false);
    }
    match app.backend.remove_loopback(id) {
        Ok(()) => {
            app.sync_endpoints();
            app.status = "Removed Forge cable".into();
        }
        Err(err) => app.status = format!("Remove failed: {err}"),
    }
}
