#![windows_subsystem = "windows"]

mod app;
mod mpe;
mod script;
mod sysex;
mod thru;

use midi_forge_io::{Direction, MidiBackend, default_backend};

fn main() -> eframe::Result<()> {
    if std::env::args().any(|a| a == "--list") {
        attach_parent_console();
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
                println!(
                    "{dir}  {:<6}  {:<18}  {}",
                    ep.protocol.label(),
                    ep.id.0,
                    ep.name
                );
            }
        }
        Err(err) => {
            eprintln!("failed to enumerate MIDI endpoints: {err}");
            std::process::exit(1);
        }
    }
}

/// GUI subsystem binaries have no console. Attach the parent terminal for `--list`.
fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn AttachConsole(dw_process_id: u32) -> i32;
        }

        #[repr(C)]
        struct CFile {
            _private: [u8; 0],
        }

        unsafe extern "C" {
            fn __acrt_iob_func(index: u32) -> *mut CFile;
            fn freopen_s(
                stream: *mut *mut CFile,
                filename: *const u8,
                mode: *const u8,
                old: *mut CFile,
            ) -> i32;
        }

        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let mut unused = std::ptr::null_mut();
        let _ = freopen_s(
            &mut unused,
            c"CONOUT$".as_ptr().cast(),
            c"w".as_ptr().cast(),
            __acrt_iob_func(1),
        );
        let _ = freopen_s(
            &mut unused,
            c"CONOUT$".as_ptr().cast(),
            c"w".as_ptr().cast(),
            __acrt_iob_func(2),
        );
        let _ = freopen_s(
            &mut unused,
            c"CONIN$".as_ptr().cast(),
            c"r".as_ptr().cast(),
            __acrt_iob_func(0),
        );
    }
}
