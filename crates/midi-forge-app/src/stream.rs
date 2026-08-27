use eframe::egui;

use midi_forge_core::{EndpointStream, PROTOCOL_MIDI1, PROTOCOL_MIDI2, fb_direction_label};
use midi_forge_io::ProtocolHint;

use crate::app::EngineInner;

pub fn stream_panel(ui: &mut egui::Ui, app: &mut EngineInner) {
    ui.separator();
    ui.heading("Stream");
    ui.weak("UMP type 0xF — sent on UMP open, not WinMM.");

    let Some(id) = selected_ump_id(app) else {
        ui.weak("Select or open a UMP endpoint.");
        return;
    };
    let name = app
        .endpoints
        .iter()
        .find(|e| e.id.0 == id)
        .map(|e| e.name.as_str())
        .unwrap_or(id.as_str());
    ui.label(format!("Endpoint  {name}"));

    let Some(port) = app.port_by_endpoint.get(&id).copied() else {
        ui.weak("Open this endpoint to discover stream info.");
        return;
    };
    let Some(tracker) = app.stream.get(&port) else {
        ui.weak("Waiting for Endpoint Info / names.");
        return;
    };
    snapshot_ui(ui, tracker.snapshot());
}

fn snapshot_ui(ui: &mut egui::Ui, snap: &EndpointStream) {
    let proto = match snap.protocol {
        PROTOCOL_MIDI2 => "MIDI 2".to_string(),
        PROTOCOL_MIDI1 => "MIDI 1".to_string(),
        0 => "—".to_string(),
        n => n.to_string(),
    };
    if !snap.name.is_empty() {
        ui.strong(&snap.name);
    }
    ui.label(format!("Protocol  {proto}"));
    ui.label(format!(
        "JR  tx {}  rx {}",
        if snap.jr_tx { "yes" } else { "no" },
        if snap.jr_rx { "yes" } else { "no" },
    ));
    if !snap.product_id.is_empty() {
        ui.weak(format!("Product  {}", snap.product_id));
    }
    if let Some(id) = &snap.identity {
        ui.weak(format!(
            "Identity  {:02X}:{:02X}:{:02X} family {} model {}",
            id.manufacturer[0], id.manufacturer[1], id.manufacturer[2], id.family, id.model
        ));
    }
    if snap.blocks.is_empty() {
        ui.weak("No function blocks yet.");
        return;
    }
    ui.add_space(4.0);
    ui.strong("Function blocks");
    egui::Grid::new("stream_fb_table")
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            ui.weak("id");
            ui.weak("groups");
            ui.weak("MIDI");
            ui.weak("dir");
            ui.weak("name");
            ui.end_row();
            for b in &snap.blocks {
                ui.monospace(format!("{}", b.id));
                ui.label(format!("{}+{}", b.first_group, b.n_groups));
                ui.label(match (b.midi1, b.midi2) {
                    (true, true) => "1+2",
                    (true, false) => "1",
                    (false, true) => "2",
                    (false, false) => "—",
                });
                ui.label(fb_direction_label(b.direction));
                ui.label(if b.name.is_empty() { "—" } else { &b.name });
                ui.end_row();
            }
        });
}

fn selected_ump_id(app: &EngineInner) -> Option<String> {
    if let Some(id) = &app.selected_endpoint {
        if app
            .endpoints
            .iter()
            .any(|e| e.id.0 == *id && e.protocol == ProtocolHint::Ump)
        {
            return Some(id.clone());
        }
    }
    app.endpoints
        .iter()
        .find(|e| {
            e.protocol == ProtocolHint::Ump
                && (app.open_inputs.contains(&e.id.0) || app.open_outputs.contains(&e.id.0))
        })
        .map(|e| e.id.0.clone())
}

pub fn endpoint_stream_line(app: &EngineInner, endpoint_id: &str) -> Option<String> {
    let port = app.port_by_endpoint.get(endpoint_id)?;
    let snap = app.stream.get(port)?.snapshot();
    if snap.name.is_empty() && snap.protocol == 0 && snap.blocks.is_empty() {
        return None;
    }
    let proto = match snap.protocol {
        PROTOCOL_MIDI2 => "MIDI 2",
        PROTOCOL_MIDI1 => "MIDI 1",
        _ => "UMP",
    };
    let name = if snap.name.is_empty() {
        proto.to_string()
    } else {
        format!("{} · {proto}", snap.name)
    };
    Some(format!(
        "{name}  JR {}/{}  {} FB",
        if snap.jr_tx { "tx" } else { "—" },
        if snap.jr_rx { "rx" } else { "—" },
        snap.blocks.len()
    ))
}
