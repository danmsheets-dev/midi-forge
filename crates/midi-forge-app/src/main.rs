mod app;
mod sysex;
mod thru;

use midi_forge_io::{Direction, MidiBackend, default_backend};

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--list") {
        list_ports();
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Midi-Forge"),
        ..Default::default()
    };

    eframe::run_native(
        "Midi-Forge",
        options,
        Box::new(|cc| Ok(Box::new(app::MidiForgeApp::new(cc)))),
    )
}

fn list_ports() {
    let mut backend: Box<dyn MidiBackend> = default_backend();
    match backend.refresh() {
        Ok(()) => {
            println!("backend: {}", backend.name());
            if backend.endpoints().is_empty() {
                println!("(no MIDI endpoints)");
                return;
            }
            for ep in backend.endpoints() {
                let dir = match ep.direction {
                    Direction::Input => "in ",
                    Direction::Output => "out",
                    Direction::Bidirectional => "bidi",
                };
                println!("{dir}  {:<16}  {}", ep.id.0, ep.name);
            }
        }
        Err(err) => {
            eprintln!("failed to enumerate MIDI endpoints: {err}");
            std::process::exit(1);
        }
    }
}
