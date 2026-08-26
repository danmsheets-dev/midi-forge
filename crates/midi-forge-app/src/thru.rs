use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};

use midi_forge_core::{
    DataMap, MapAction, MapEntry, MatchKind, Matcher, PortId, ValueMap, VoiceKind,
};
use midi_forge_io::{Direction, Endpoint};

use crate::app::{MidiForgeApp, truncate};

pub fn thru_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.horizontal(|ui| {
        ui.heading("Thru");
        ui.weak("Cables and matrix are the same graph. Maps run after filters.");
    });
    ui.separator();

    let inputs: Vec<Endpoint> = app
        .endpoints
        .iter()
        .filter(|e| e.direction == Direction::Input)
        .cloned()
        .collect();
    let outputs: Vec<Endpoint> = app
        .endpoints
        .iter()
        .filter(|e| e.direction == Direction::Output)
        .cloned()
        .collect();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            patchbay(ui, app, &inputs, &outputs);
        });
        ui.separator();
        ui.vertical(|ui| {
            matrix(ui, app, &inputs, &outputs);
        });
        ui.separator();
        ui.vertical(|ui| {
            egui::ScrollArea::vertical()
                .id_salt("link_editor")
                .show(ui, |ui| {
                    filter_editor(ui, app);
                    ui.add_space(8.0);
                    ui.separator();
                    map_editor(ui, app);
                });
        });
    });
}

fn patchbay(ui: &mut egui::Ui, app: &mut MidiForgeApp, inputs: &[Endpoint], outputs: &[Endpoint]) {
    ui.label("Patchbay");
    if inputs.is_empty() || outputs.is_empty() {
        ui.weak("Need an input and an output.");
        return;
    }
    let row_h = 22.0;
    let rows = inputs.len().max(outputs.len()) as f32;
    let size = Vec2::new(300.0, (rows * row_h + 20.0).max(80.0));
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    let left = rect.left() + 92.0;
    let right = rect.right() - 12.0;

    let mut in_pos: Vec<(PortId, Pos2)> = Vec::new();
    for (i, inp) in inputs.iter().enumerate() {
        let y = rect.top() + 12.0 + i as f32 * row_h;
        let port = app.ensure_port(&inp.id);
        let pos = Pos2::new(left, y);
        in_pos.push((port, pos));
        painter.text(
            Pos2::new(rect.left() + 4.0, y - 6.0),
            egui::Align2::LEFT_TOP,
            truncate(&inp.name, 14),
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
        painter.circle_filled(pos, 4.5, Color32::from_rgb(70, 170, 255));
    }

    let mut out_pos: Vec<(PortId, Pos2)> = Vec::new();
    for (i, out) in outputs.iter().enumerate() {
        let y = rect.top() + 12.0 + i as f32 * row_h;
        let port = app.ensure_port(&out.id);
        let pos = Pos2::new(right, y);
        out_pos.push((port, pos));
        painter.text(
            Pos2::new(right - 8.0, y - 6.0),
            egui::Align2::RIGHT_TOP,
            truncate(&out.name, 14),
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
        painter.circle_filled(pos, 4.5, Color32::from_rgb(255, 170, 70));
    }

    for link in app.router.links() {
        let Some(from) = in_pos
            .iter()
            .find(|(p, _)| *p == link.from)
            .map(|(_, p)| *p)
        else {
            continue;
        };
        let Some(to) = out_pos.iter().find(|(p, _)| *p == link.to).map(|(_, p)| *p) else {
            continue;
        };
        let selected = app.selected_link == Some((link.from, link.to));
        let color = if selected {
            Color32::from_rgb(255, 210, 80)
        } else {
            Color32::from_rgb(140, 150, 170)
        };
        painter.line_segment(
            [from, to],
            Stroke::new(if selected { 3.0 } else { 2.0 }, color),
        );
    }
}

fn matrix(ui: &mut egui::Ui, app: &mut MidiForgeApp, inputs: &[Endpoint], outputs: &[Endpoint]) {
    ui.label("Matrix");
    if inputs.is_empty() || outputs.is_empty() {
        ui.label("Need at least one input and one output.");
        return;
    }
    egui::Grid::new("thru_matrix").striped(true).show(ui, |ui| {
        ui.label("");
        for out in outputs {
            ui.strong(truncate(&out.name, 12));
        }
        ui.end_row();
        for inp in inputs {
            ui.strong(truncate(&inp.name, 14));
            for out in outputs {
                let from = app.ensure_port(&inp.id);
                let to = app.ensure_port(&out.id);
                let mut on = app.router.is_linked(from, to);
                let selected = app.selected_link == Some((from, to));
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, "").changed()
                        && let Err(err) = app.set_thru(&inp.id, &out.id, on)
                    {
                        app.port_errors.insert(out.id.0.clone(), err);
                    }
                    if selected {
                        ui.weak("●");
                    } else if ui.small_button("edit").clicked() && app.router.is_linked(from, to) {
                        app.selected_link = Some((from, to));
                    }
                });
            }
            ui.end_row();
        }
    });
}

fn filter_editor(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    let Some((from, to)) = app.selected_link else {
        ui.weak("Select a thru cell to edit filter and maps.");
        return;
    };
    let Some(mut filter) = app.router.filter(from, to).cloned() else {
        ui.weak("That cell is not connected.");
        return;
    };

    ui.strong(link_title(app, from, to));
    ui.add_space(4.0);
    ui.label("Filter — pass");
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut filter.notes, "Notes");
        ui.checkbox(&mut filter.poly_pressure, "Poly press");
        ui.checkbox(&mut filter.control_change, "CC");
        ui.checkbox(&mut filter.program_change, "Program");
        ui.checkbox(&mut filter.channel_pressure, "Chan press");
        ui.checkbox(&mut filter.pitch_bend, "Bend");
        ui.checkbox(&mut filter.sysex, "SysEx");
        ui.checkbox(&mut filter.clock, "Clock");
        ui.checkbox(&mut filter.transport, "Start/Stop");
        ui.checkbox(&mut filter.active_sensing, "Sensing");
        ui.checkbox(&mut filter.reset, "Reset");
        ui.checkbox(&mut filter.system_common, "Sys common");
    });
    ui.horizontal(|ui| {
        ui.label("Channels");
        if ui.small_button("All").clicked() {
            filter.set_all_channels(true);
        }
        if ui.small_button("None").clicked() {
            filter.set_all_channels(false);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for ch in 0..16u8 {
            let mut on = filter.channel_enabled(ch);
            if ui.checkbox(&mut on, format!("{}", ch + 1)).changed() {
                filter.set_channel_enabled(ch, on);
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Force channel");
        let mut ch = i32::from(filter.force_channel.map(|c| c + 1).unwrap_or(0));
        if ui
            .add(egui::DragValue::new(&mut ch).range(0..=16).prefix("Ch "))
            .changed()
        {
            filter.force_channel = if ch == 0 {
                None
            } else {
                Some((ch as u8).saturating_sub(1))
            };
        }
        ui.weak("0 = keep");
    });

    app.router.set_filter(from, to, filter);
}

fn map_editor(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    let Some((from, to)) = app.selected_link else {
        return;
    };
    let Some(mut map) = app.router.map(from, to).cloned() else {
        return;
    };

    ui.label("Data map — first match wins");
    ui.checkbox(&mut map.pass_unmatched, "Pass unmatched channel-voice");
    ui.horizontal_wrapped(|ui| {
        let learning = app.learn == Some((from, to));
        let learn_label = if learning { "Listening…" } else { "Learn" };
        if ui
            .button(learn_label)
            .on_hover_text("Next CC or note on this input fills a matcher row")
            .clicked()
        {
            app.learn = if learning { None } else { Some((from, to)) };
            app.status = if app.learn.is_some() {
                "MIDI learn: play a CC or note".into()
            } else {
                "Learn cancelled".into()
            };
        }
        if ui.button("Add row").clicked() {
            map.entries.push(MapEntry {
                matcher: Matcher::default(),
                action: MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Keep,
                    data2: ValueMap::Keep,
                },
            });
        }
        if ui.button("Transpose +12").clicked() {
            map.entries.extend(DataMap::transpose(12).entries);
        }
        if ui.button("Transpose -12").clicked() {
            map.entries.extend(DataMap::transpose(-12).entries);
        }
        if ui.button("Invert vel").clicked() {
            map.entries.extend(DataMap::invert_velocity().entries);
        }
        if ui.button("CC1→CC7").clicked() {
            map.entries.extend(DataMap::remap_cc(1, 7).entries);
        }
    });

    let mut remove = None;
    for (i, entry) in map.entries.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{}.", i + 1));
                match_kind_combo(ui, i, &mut entry.matcher.kind);
                ui.label("d1");
                ui.add(egui::DragValue::new(&mut entry.matcher.data1_min).range(0..=127));
                ui.label("–");
                ui.add(egui::DragValue::new(&mut entry.matcher.data1_max).range(0..=127));
                ui.label("d2");
                ui.add(egui::DragValue::new(&mut entry.matcher.data2_min).range(0..=127));
                ui.label("–");
                ui.add(egui::DragValue::new(&mut entry.matcher.data2_max).range(0..=127));
                if ui.small_button("x").clicked() {
                    remove = Some(i);
                }
            });
            action_editor(ui, i, entry);
        });
    }
    if let Some(i) = remove {
        map.entries.remove(i);
    }

    app.router.set_map(from, to, map);
}

fn match_kind_combo(ui: &mut egui::Ui, row: usize, kind: &mut MatchKind) {
    egui::ComboBox::from_id_salt(("match_kind", row))
        .selected_text(kind.label())
        .width(90.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(kind, MatchKind::AnyChannelVoice, "Any voice");
            ui.selectable_value(kind, MatchKind::Notes, "Notes");
            for vk in VoiceKind::all() {
                ui.selectable_value(kind, MatchKind::One(vk), vk.label());
            }
        });
}

fn action_editor(ui: &mut egui::Ui, row: usize, entry: &mut MapEntry) {
    let mut drop = matches!(entry.action, MapAction::Drop);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut drop, "Drop").changed() {
            entry.action = if drop {
                MapAction::Drop
            } else {
                MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Keep,
                    data2: ValueMap::Keep,
                }
            };
        }
        if let MapAction::Rewrite {
            kind,
            channel,
            data1,
            data2,
        } = &mut entry.action
        {
            rewrite_kind_combo(ui, row, kind);
            let mut ch = i32::from(channel.map(|c| c + 1).unwrap_or(0));
            ui.label("ch");
            if ui
                .add(egui::DragValue::new(&mut ch).range(0..=16))
                .changed()
            {
                *channel = if ch == 0 {
                    None
                } else {
                    Some((ch as u8).saturating_sub(1))
                };
            }
            ui.label("d1");
            value_map_editor(ui, row, 1, data1);
            ui.label("d2");
            value_map_editor(ui, row, 2, data2);
        }
    });
}

fn rewrite_kind_combo(ui: &mut egui::Ui, row: usize, kind: &mut Option<VoiceKind>) {
    let text = kind.map(VoiceKind::label).unwrap_or("Keep type");
    egui::ComboBox::from_id_salt(("out_kind", row))
        .selected_text(text)
        .width(90.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(kind, None, "Keep type");
            for vk in VoiceKind::all() {
                ui.selectable_value(kind, Some(vk), vk.label());
            }
        });
}

fn value_map_editor(ui: &mut egui::Ui, row: usize, which: u8, value: &mut ValueMap) {
    let label = match value {
        ValueMap::Keep => "Keep",
        ValueMap::Constant(_) => "Const",
        ValueMap::Offset(_) => "Offset",
        ValueMap::Scale { .. } => "Scale",
    };
    egui::ComboBox::from_id_salt(("vmap", row, which))
        .selected_text(label)
        .width(70.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(matches!(value, ValueMap::Keep), "Keep")
                .clicked()
            {
                *value = ValueMap::Keep;
            }
            if ui
                .selectable_label(matches!(value, ValueMap::Constant(_)), "Const")
                .clicked()
            {
                *value = ValueMap::Constant(0);
            }
            if ui
                .selectable_label(matches!(value, ValueMap::Offset(_)), "Offset")
                .clicked()
            {
                *value = ValueMap::Offset(0);
            }
            if ui
                .selectable_label(matches!(value, ValueMap::Scale { .. }), "Scale")
                .clicked()
            {
                *value = ValueMap::Scale {
                    in_min: 0,
                    in_max: 127,
                    out_min: 0,
                    out_max: 127,
                    invert: false,
                };
            }
        });
    match value {
        ValueMap::Keep => {}
        ValueMap::Constant(v) => {
            ui.add(egui::DragValue::new(v).range(0..=127));
        }
        ValueMap::Offset(d) => {
            ui.add(egui::DragValue::new(d).range(-127..=127));
        }
        ValueMap::Scale {
            in_min,
            in_max,
            out_min,
            out_max,
            invert,
        } => {
            ui.add(egui::DragValue::new(in_min).range(0..=127).prefix("in "));
            ui.add(egui::DragValue::new(in_max).range(0..=127));
            ui.add(egui::DragValue::new(out_min).range(0..=127).prefix("out "));
            ui.add(egui::DragValue::new(out_max).range(0..=127));
            ui.checkbox(invert, "inv");
        }
    }
}

fn link_title(app: &MidiForgeApp, from: PortId, to: PortId) -> String {
    let from_name = app
        .port_names
        .get(&from)
        .cloned()
        .unwrap_or_else(|| format!("{}", from.0));
    let to_name = app
        .port_names
        .get(&to)
        .cloned()
        .unwrap_or_else(|| format!("{}", to.0));
    format!("{from_name}  →  {to_name}")
}
