use std::collections::HashSet;

use eframe::egui;
use midi_forge_core::UmpMessage;
use midi_forge_io::{Direction, EndpointId};

use crate::app::MidiForgeApp;

const WHITE: [u8; 14] = [0, 2, 4, 5, 7, 9, 11, 12, 14, 16, 17, 19, 21, 23];
const BLACK_AT: [(usize, u8); 10] = [
    (0, 1),
    (1, 3),
    (3, 6),
    (4, 8),
    (5, 10),
    (7, 13),
    (8, 15),
    (10, 18),
    (11, 20),
    (12, 22),
];

pub fn inject_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.horizontal(|ui| {
        ui.heading("Inject");
        ui.weak("Sends to the selected open output.");
        ui.separator();
        ui.label("Ch");
        ui.add(egui::DragValue::new(&mut app.inject_channel).range(1..=16));
        ui.label("Oct");
        ui.add(egui::DragValue::new(&mut app.inject_octave).range(-2..=4));
        ui.label("Vel");
        ui.add(egui::DragValue::new(&mut app.inject_velocity).range(1..=127));
    });

    let outputs: Vec<(String, String)> = app
        .endpoints
        .iter()
        .filter(|e| e.direction == Direction::Output)
        .map(|e| (e.id.0.clone(), e.name.clone()))
        .collect();
    if app.inject_dest.is_none() {
        app.inject_dest = app
            .open_outputs
            .iter()
            .next()
            .cloned()
            .or_else(|| outputs.first().map(|(id, _)| id.clone()));
    }
    ui.horizontal(|ui| {
        ui.label("Out");
        let current = app
            .inject_dest
            .as_ref()
            .and_then(|id| outputs.iter().find(|(oid, _)| oid == id))
            .map(|(_, n)| n.as_str())
            .unwrap_or("(none)");
        egui::ComboBox::from_id_salt("inject_dest")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in &outputs {
                    ui.selectable_value(&mut app.inject_dest, Some(id.clone()), name);
                }
            });
    });

    piano(ui, app);

    ui.horizontal(|ui| {
        ui.label("CC");
        ui.add(egui::DragValue::new(&mut app.inject_cc).range(0..=127));
        ui.weak(midi_forge_core::cc_label(app.inject_cc));
        let mut val = app.inject_cc_val;
        if ui
            .add(egui::Slider::new(&mut val, 0..=127).text("value"))
            .changed()
        {
            app.inject_cc_val = val;
            send_cc(app, val);
        }
        if ui.button("Send CC").clicked() {
            send_cc(app, app.inject_cc_val);
        }
    });
}

fn piano(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    let white_w = 22.0;
    let white_h = 72.0;
    let black_w = 14.0;
    let black_h = 44.0;
    let origin = ui.cursor().min;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(WHITE.len() as f32 * white_w, white_h),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);

    let base = ((app.inject_octave + 2) * 12 + 36).clamp(0, 96) as u8;
    let mut pressed: HashSet<u8> = HashSet::new();

    for (i, off) in WHITE.iter().enumerate() {
        let note = base.saturating_add(*off);
        let x = origin.x + i as f32 * white_w;
        let r =
            egui::Rect::from_min_size(egui::pos2(x, origin.y), egui::vec2(white_w - 1.0, white_h));
        let id = ui.id().with(("w", note));
        let resp = ui.interact(r, id, egui::Sense::click_and_drag());
        let down = resp.is_pointer_button_down_on();
        painter.rect_filled(
            r,
            1.0,
            if down {
                egui::Color32::from_rgb(180, 210, 255)
            } else {
                egui::Color32::from_rgb(240, 240, 245)
            },
        );
        painter.rect_stroke(
            r,
            1.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 80, 90)),
            egui::StrokeKind::Inside,
        );
        if down {
            pressed.insert(note);
        }
    }

    for (white_i, off) in BLACK_AT {
        let note = base.saturating_add(off);
        let x = origin.x + (white_i as f32 + 1.0) * white_w - black_w / 2.0;
        let r = egui::Rect::from_min_size(egui::pos2(x, origin.y), egui::vec2(black_w, black_h));
        let id = ui.id().with(("b", note));
        let resp = ui.interact(r, id, egui::Sense::click_and_drag());
        let down = resp.is_pointer_button_down_on();
        painter.rect_filled(
            r,
            1.0,
            if down {
                egui::Color32::from_rgb(80, 120, 180)
            } else {
                egui::Color32::from_rgb(30, 30, 36)
            },
        );
        if down {
            pressed.insert(note);
        }
    }

    let held = app.held_keys.clone();
    for note in pressed.difference(&held) {
        send_note(app, *note, true);
    }
    for note in held.difference(&pressed) {
        send_note(app, *note, false);
    }
    app.held_keys = pressed;
}

fn send_note(app: &mut MidiForgeApp, note: u8, on: bool) {
    let ch = app.inject_channel.saturating_sub(1).min(15);
    let vel = if on { app.inject_velocity.min(127) } else { 0 };
    let status = if on { 0x90 } else { 0x80 } | ch;
    let packet = UmpMessage::midi1_channel_voice(0, status, note.min(127), vel);
    send_inject(app, packet);
}

fn send_cc(app: &mut MidiForgeApp, value: u8) {
    let ch = app.inject_channel.saturating_sub(1).min(15);
    let packet =
        UmpMessage::midi1_channel_voice(0, 0xB0 | ch, app.inject_cc.min(127), value.min(127));
    send_inject(app, packet);
}

fn send_inject(app: &mut MidiForgeApp, packet: UmpMessage) {
    let Some(dest) = app.inject_dest.clone() else {
        app.status = "Pick an inject output".into();
        return;
    };
    let id = EndpointId(dest);
    if let Err(err) = app.set_output_open(&id, true) {
        app.port_errors.insert(id.0.clone(), err);
        return;
    }
    app.hang.push(&packet);
    app.live.push(&packet);
    let _ = app.nrpn.push(&packet);
    if let Err(err) = app.send_packet(&id, &packet) {
        app.status = format!("Inject failed: {err}");
    }
}
