use eframe::egui;

use crate::app::MidiForgeApp;

pub fn clock_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.horizontal(|ui| {
        ui.heading("Clock");
        ui.weak("Host receive — not cable delay.");
        if ui.small_button("Reset").clicked() {
            app.clock.reset();
        }
    });
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

pub fn route_panel(ui: &mut egui::Ui, app: &MidiForgeApp) {
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
