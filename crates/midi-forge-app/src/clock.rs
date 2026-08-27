use eframe::egui;

use crate::app::EngineInner;

pub fn clock_panel(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.horizontal(|ui| {
        ui.heading("Clock");
        ui.weak("Host receive — not cable delay.");
        if ui.small_button("Reset").clicked() {
            app.clock.reset();
        }
    });
    master_row(ui, app);
    ui.separator();
    let runaway = app.clock.runaway();
    let summary = app.clock.summary();
    if runaway {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), summary);
    } else {
        ui.weak(summary);
    }
    hist_row(ui, "clk", app.clock.clock.bins(16));
    hist_row(ui, "nt", app.clock.notes.bins(16));
    if let Some(note_mean) = app.clock.notes.mean_ns() {
        ui.weak(format!(
            "note-on gap {} ms  jitter {} µs",
            note_mean / 1_000_000,
            app.clock.notes.jitter_ns().unwrap_or(0) / 1000
        ));
    }
}

fn master_row(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.horizontal_wrapped(|ui| {
        ui.strong("Master");
        ui.checkbox(&mut app.master.enabled, "Enable")
            .on_hover_text("Generate MIDI clock on the selected output (engine thread)");
        let mut bpm = app.master.bpm;
        if ui
            .add(
                egui::DragValue::new(&mut bpm)
                    .range(20.0..=300.0)
                    .prefix("BPM "),
            )
            .changed()
        {
            app.master.set_bpm(bpm);
        }
        let outputs: Vec<(String, String)> = app
            .endpoints
            .iter()
            .filter(|e| e.direction == midi_forge_io::Direction::Output)
            .map(|e| (e.id.0.clone(), e.name.clone()))
            .collect();
        if app.master_dest.is_none() {
            app.master_dest = outputs.first().map(|(id, _)| id.clone());
        }
        let label = app
            .master_dest
            .as_ref()
            .and_then(|id| {
                outputs
                    .iter()
                    .find(|(oid, _)| oid == id)
                    .map(|(_, n)| n.as_str())
            })
            .unwrap_or("(none)");
        egui::ComboBox::from_id_salt("clock_master_dest")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (id, name) in &outputs {
                    ui.selectable_value(&mut app.master_dest, Some(id.clone()), name);
                }
            });
        if ui.button("Start").clicked() {
            let start = app.master.start(app.host_ns());
            send_master(app, start);
        }
        if ui.button("Continue").clicked() {
            let pkt = app.master.cont(app.host_ns());
            send_master(app, pkt);
        }
        if ui.button("Stop").clicked() {
            let pkt = app.master.stop();
            send_master(app, pkt);
        }
        if app.master.running() {
            ui.colored_label(
                egui::Color32::from_rgb(80, 180, 140),
                format!("{} ticks", app.master.ticks),
            );
        }
    });
}

fn send_master(app: &mut EngineInner, packet: midi_forge_core::UmpMessage) {
    let Some(dest) = app.master_dest.clone() else {
        app.status = "Pick a clock master output".into();
        return;
    };
    let id = midi_forge_io::EndpointId(dest);
    if let Err(err) = app.set_output_open(&id, true) {
        app.port_errors.insert(id.0.clone(), err);
        return;
    }
    if let Err(err) = app.send_packet(&id, &packet) {
        app.status = format!("Clock master: {err}");
    }
}

fn hist_row(ui: &mut egui::Ui, label: &str, bins: Vec<usize>) {
    ui.horizontal(|ui| {
        ui.monospace(format!("{label:>3}"));
        let max = bins.iter().copied().max().unwrap_or(0).max(1);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(240.0, 16.0), egui::Sense::hover());
        let w = rect.width() / bins.len().max(1) as f32;
        for (i, c) in bins.iter().enumerate() {
            let h = rect.height() * (*c as f32 / max as f32);
            let mut bar = egui::Rect::from_min_size(
                egui::pos2(rect.left() + i as f32 * w, rect.bottom() - h),
                egui::vec2(w.max(1.0) - 1.0, h),
            );
            if bar.height() < 1.0 && *c > 0 {
                bar.set_top(rect.bottom() - 1.0);
            }
            ui.painter().rect_filled(
                bar,
                0.0,
                if *c > 0 {
                    egui::Color32::from_rgb(70, 160, 255)
                } else {
                    egui::Color32::from_rgb(40, 40, 48)
                },
            );
        }
    });
}

pub fn route_panel(ui: &mut egui::Ui, app: &EngineInner) {
    ui.horizontal(|ui| {
        ui.heading("Thru path");
        ui.weak("In → out. Fan-out is a split.");
    });
    egui::ScrollArea::vertical()
        .max_height(72.0)
        .id_salt("route_log")
        .show(ui, |ui| {
            if app.routes.is_empty() {
                ui.weak("No thru yet.");
                return;
            }
            for ev in app.routes.iter().rev().take(12) {
                let from = app
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
                            app.port_names
                                .get(p)
                                .cloned()
                                .unwrap_or_else(|| format!("p{}", p.0))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let sum = midi_forge_core::decode(&ev.packet).summary();
                ui.monospace(format!("{from} → {dests}  {sum}"));
            }
        });
}
