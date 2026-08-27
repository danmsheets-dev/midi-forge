use eframe::egui;
use midi_forge_core::cc_label;

use crate::app::EngineInner;

pub fn live_panel(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.horizontal(|ui| {
        ui.heading("Live");
        ui.weak("Now — not the log. Notes / last CC / bend per channel.");
        if ui.small_button("Reset").clicked() {
            app.live = midi_forge_core::LiveView::new();
        }
        if ui.small_button("Snapshot").clicked() {
            ui.ctx().copy_text(app.snapshot_text());
            app.status = "Snapshot copied".into();
        }
    });
    let row_h = 16.0;
    egui::ScrollArea::vertical()
        .max_height(16.0 * 9.0)
        .id_salt("live_view")
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.monospace(egui::RichText::new("Ch").strong());
                ui.monospace(egui::RichText::new("Note").strong());
                ui.monospace(egui::RichText::new("n").strong());
                ui.monospace(egui::RichText::new("Prog").strong());
                ui.monospace(egui::RichText::new("CC").strong());
                ui.monospace(egui::RichText::new("Bend").strong());
            });
            for (i, ch) in app.live.ch.iter().enumerate() {
                if !ch.dirty && ch.sounding == 0 && ch.last_cc.is_none() {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.monospace(format!("{:>2}", i + 1));
                    let note = ch
                        .last_note
                        .map(|n| format!("{n:>3}"))
                        .unwrap_or_else(|| "  —".into());
                    ui.monospace(note);
                    let n_col = if ch.sounding > 0 {
                        egui::Color32::from_rgb(80, 220, 120)
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    ui.colored_label(n_col, format!("{:>2}", ch.sounding));
                    ui.monospace(format!("{:>3}", ch.program));
                    if let Some((cc, val)) = ch.last_cc {
                        ui.monospace(format!("{} {val}", cc_label(cc)));
                    } else {
                        ui.weak("—");
                    }
                    ui.monospace(format!("{:>5}", ch.bend));
                    let frac = f32::from(ch.last_vel) / 127.0;
                    let (bar, _) =
                        ui.allocate_exact_size(egui::vec2(40.0, row_h - 4.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(bar, 1.0, egui::Color32::from_rgb(40, 40, 48));
                    let mut fill = bar;
                    fill.set_width(bar.width() * frac);
                    if ch.sounding > 0 {
                        ui.painter()
                            .rect_filled(fill, 1.0, egui::Color32::from_rgb(70, 160, 255));
                    }
                });
            }
        });
    if let Some(p) = app.nrpn.last() {
        ui.weak(p.summary());
    }
}
