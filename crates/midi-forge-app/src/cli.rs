//! Headless MIDI send/receive. Same binary as the GUI.

use std::thread;
use std::time::{Duration, Instant};

use midi_forge_core::{
    ClockMaster, IDENTITY_REQUEST, MidiEvent, UmpMessage, decode, midi2_cc, midi2_note_on,
    panic_packets, value7_to_32, velocity7_to_16,
};
use midi_forge_io::{Direction, Endpoint, MidiBackend, default_backend};

pub fn dispatch(args: &[String]) -> bool {
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    matches!(
        cmd,
        "--help" | "-h" | "help" | "send" | "receive" | "identity" | "panic" | "clock"
    )
}

pub fn run(args: &[String]) -> i32 {
    match args.get(1).map(String::as_str).unwrap_or("help") {
        "--help" | "-h" | "help" => {
            print_help();
            0
        }
        "send" => cmd_send(args),
        "receive" => cmd_receive(args),
        "identity" => cmd_identity(args),
        "panic" => cmd_panic(args),
        "clock" => cmd_clock(args),
        other => {
            eprintln!("unknown command {other:?}");
            print_help();
            2
        }
    }
}

fn print_help() {
    eprintln!(
        "Midi-Forge CLI
  midi-forge --list
  midi-forge send --out <name> note <note> <vel> [--ch N] [--m2]
  midi-forge send --out <name> cc <cc> <val> [--ch N] [--m2]
  midi-forge identity --out <name>
  midi-forge panic --out <name>
  midi-forge receive --in <name> [--seconds N]
  midi-forge clock --out <name> [--bpm 120] [--seconds 2]
"
    );
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].as_str())
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn open_backend() -> Box<dyn MidiBackend> {
    default_backend()
}

fn find_ep(eps: &[Endpoint], dir: Direction, needle: &str) -> Result<Endpoint, String> {
    let n = needle.to_ascii_lowercase();
    eps.iter()
        .filter(|e| e.direction == dir)
        .find(|e| {
            e.name.to_ascii_lowercase().contains(&n) || e.id.0.to_ascii_lowercase().contains(&n)
        })
        .cloned()
        .ok_or_else(|| format!("no {dir:?} matching {needle:?}"))
}

fn cmd_send(args: &[String]) -> i32 {
    let Some(out_n) = flag(args, "--out") else {
        eprintln!("send requires --out <name>");
        return 2;
    };
    let positional: Vec<&str> = args
        .iter()
        .skip(2)
        .map(String::as_str)
        .filter(|a| !a.starts_with('-') && *a != out_n)
        .filter(|a| {
            !matches!(*a, "note" | "cc" | "send")
                && flag(args, "--out") != Some(a)
                && flag(args, "--ch") != Some(a)
        })
        .collect();
    let kind = args
        .iter()
        .skip(2)
        .map(String::as_str)
        .find(|a| *a == "note" || *a == "cc");
    let mut backend = open_backend();
    if let Err(err) = backend.refresh() {
        eprintln!("{err}");
        return 1;
    }
    let ep = match find_ep(backend.endpoints(), Direction::Output, out_n) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    if let Err(err) = backend.open_output(&ep.id, midi_forge_core::PortId(1)) {
        eprintln!("{err}");
        return 1;
    }
    let ch = flag(args, "--ch")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1)
        .saturating_sub(1)
        .min(15);
    let m2 = has(args, "--m2");
    let result = match kind {
        Some("note") => {
            let note: u8 = positional
                .first()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);
            let vel: u8 = positional
                .get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);
            let pkt = if m2 {
                midi2_note_on(0, ch, note, velocity7_to_16(vel))
            } else {
                UmpMessage::midi1_channel_voice(0, 0x90 | ch, note, vel)
            };
            backend.send(&ep.id, &pkt)
        }
        Some("cc") => {
            let cc: u8 = positional.first().and_then(|s| s.parse().ok()).unwrap_or(1);
            let val: u8 = positional
                .get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(127);
            let pkt = if m2 {
                midi2_cc(0, ch, cc, value7_to_32(val))
            } else {
                UmpMessage::midi1_channel_voice(0, 0xB0 | ch, cc, val)
            };
            backend.send(&ep.id, &pkt)
        }
        _ => {
            eprintln!("send note|cc …");
            return 2;
        }
    };
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn cmd_identity(args: &[String]) -> i32 {
    let Some(out_n) = flag(args, "--out") else {
        eprintln!("identity requires --out <name>");
        return 2;
    };
    let mut backend = open_backend();
    if let Err(err) = backend.refresh() {
        eprintln!("{err}");
        return 1;
    }
    let ep = match find_ep(backend.endpoints(), Direction::Output, out_n) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    if let Err(err) = backend.open_output(&ep.id, midi_forge_core::PortId(1)) {
        eprintln!("{err}");
        return 1;
    }
    match backend.send_sysex(&ep.id, &IDENTITY_REQUEST) {
        Ok(()) => {
            println!("identity request → {}", ep.name);
            0
        }
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn cmd_panic(args: &[String]) -> i32 {
    let Some(out_n) = flag(args, "--out") else {
        eprintln!("panic requires --out <name>");
        return 2;
    };
    let mut backend = open_backend();
    if let Err(err) = backend.refresh() {
        eprintln!("{err}");
        return 1;
    }
    let ep = match find_ep(backend.endpoints(), Direction::Output, out_n) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    if let Err(err) = backend.open_output(&ep.id, midi_forge_core::PortId(1)) {
        eprintln!("{err}");
        return 1;
    }
    let mut n = 0usize;
    for p in panic_packets() {
        if backend.send(&ep.id, &p).is_ok() {
            n += 1;
        }
    }
    println!("panic: {n} messages → {}", ep.name);
    0
}

fn cmd_receive(args: &[String]) -> i32 {
    let Some(in_n) = flag(args, "--in") else {
        eprintln!("receive requires --in <name>");
        return 2;
    };
    let secs: f64 = flag(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3.0);
    let mut backend = open_backend();
    if let Err(err) = backend.refresh() {
        eprintln!("{err}");
        return 1;
    }
    let ep = match find_ep(backend.endpoints(), Direction::Input, in_n) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    if let Err(err) = backend.open_input(&ep.id, midi_forge_core::PortId(1)) {
        eprintln!("{err}");
        return 1;
    }
    println!("receiving {} for {secs}s", ep.name);
    let deadline = Instant::now() + Duration::from_secs_f64(secs.max(0.1));
    while Instant::now() < deadline {
        let mut buf: Vec<MidiEvent> = Vec::new();
        let _ = backend.poll(&mut buf);
        for ev in buf {
            println!("{}", decode(&ev.packet).summary());
        }
        thread::sleep(Duration::from_millis(5));
    }
    0
}

fn cmd_clock(args: &[String]) -> i32 {
    let Some(out_n) = flag(args, "--out") else {
        eprintln!("clock requires --out <name>");
        return 2;
    };
    let bpm: f64 = flag(args, "--bpm")
        .and_then(|s| s.parse().ok())
        .unwrap_or(120.0);
    let secs: f64 = flag(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let mut backend = open_backend();
    if let Err(err) = backend.refresh() {
        eprintln!("{err}");
        return 1;
    }
    let ep = match find_ep(backend.endpoints(), Direction::Output, out_n) {
        Ok(e) => e,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    if let Err(err) = backend.open_output(&ep.id, midi_forge_core::PortId(1)) {
        eprintln!("{err}");
        return 1;
    }
    let mut master = ClockMaster::new();
    master.set_bpm(bpm);
    let start = Instant::now();
    let pkt = master.start(0);
    let _ = backend.send(&ep.id, &pkt);
    println!("clock {bpm} BPM → {} for {secs}s", ep.name);
    while start.elapsed().as_secs_f64() < secs {
        let now = start.elapsed().as_nanos() as u64;
        for p in master.poll(now) {
            let _ = backend.send(&ep.id, &p);
        }
        thread::sleep(Duration::from_millis(1));
    }
    let _ = backend.send(&ep.id, &master.stop());
    0
}
