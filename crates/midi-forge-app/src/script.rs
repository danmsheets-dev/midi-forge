use eframe::egui;

use crate::app::MidiForgeApp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RightTab {
    Sysex,
    Lua,
}

pub fn lua_panel(ui: &mut egui::Ui, app: &mut MidiForgeApp) {
    ui.heading("Lua");
    ui.weak("Runs on captured events before thru. Monitor still shows the wire.");
    ui.separator();

    ui.horizontal(|ui| {
        let mut enabled = app.script.enabled();
        if ui.checkbox(&mut enabled, "Enable").changed() {
            app.script.set_enabled(enabled);
            app.status = if enabled {
                "Lua enabled".into()
            } else {
                "Lua disabled — thru is unfiltered by script".into()
            };
        }
        if ui.button("Apply").clicked() {
            apply_script(app);
        }
        if ui.button("Load .lua").clicked() {
            load_lua(app);
        }
        if ui.button("Save .lua").clicked() {
            save_lua(app);
        }
        if ui.button("Reset").clicked() {
            app.script.source = midi_forge_script::DEFAULT_SOURCE.to_string();
            apply_script(app);
        }
    });

    if let Some(err) = app.script.error() {
        ui.colored_label(egui::Color32::from_rgb(220, 80, 80), err);
    } else if app.script.enabled() {
        ui.weak("Compiled. on_midi is live.");
    } else {
        ui.weak("Apply compiles. Enable to run on capture.");
    }

    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("lua_editor")
        .max_height(280.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut app.script.source)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(14)
                    .font(egui::TextStyle::Monospace),
            );
        });

    ui.separator();
    ui.horizontal(|ui| {
        ui.weak("Script log");
        if ui.small_button("Clear log").clicked() {
            app.script.clear_log();
        }
    });
    let lines = app.script.log_lines();
    egui::ScrollArea::vertical()
        .id_salt("lua_log")
        .max_height(100.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if lines.is_empty() {
                ui.weak("print() / midi.log() appear here.");
            } else {
                for line in &lines {
                    ui.monospace(line);
                }
            }
        });
}

fn apply_script(app: &mut MidiForgeApp) {
    match app.script.reload() {
        Ok(()) => app.status = "Lua compiled".into(),
        Err(err) => app.status = format!("Lua: {err}"),
    }
}

fn load_lua(app: &mut MidiForgeApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Lua", &["lua"])
        .pick_file()
    else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(src) => {
            app.script.source = src;
            apply_script(app);
            app.status = format!("Loaded {}", path.display());
        }
        Err(err) => app.status = format!("Load failed: {err}"),
    }
}

fn save_lua(app: &mut MidiForgeApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Lua", &["lua"])
        .set_file_name("midi-forge.lua")
        .save_file()
    else {
        return;
    };
    match std::fs::write(&path, &app.script.source) {
        Ok(()) => app.status = format!("Saved {}", path.display()),
        Err(err) => app.status = format!("Save failed: {err}"),
    }
}

pub fn apply_profile_lua(app: &mut MidiForgeApp, source: String, enabled: bool) {
    if !source.is_empty() {
        app.script.source = source;
        let _ = app.script.reload();
    }
    app.script.set_enabled(enabled);
}
