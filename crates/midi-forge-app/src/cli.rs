//! Headless MIDI send/receive. Same binary as the GUI.

use std::thread;
use std::time::{Duration, Instant};

use midi_forge_core::{
    ClockMaster, IDENTITY_REQUEST, MidiEvent, UmpMessage, decode, midi2_cc, midi2_note_on,
    midi2_registered_controller, panic_packets, value7_to_32, velocity7_to_16,
};
use midi_forge_io::{Direction, Endpoint, MidiBackend, default_backend};

pub fn dispatch(args: &[String]) -> bool {
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    matches!(
        cmd,
        "--help" | "-h" | "help" | "send" | "receive" | "identity" | "panic" | "clock" | "mcp"
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
        "mcp" => {
            eprintln!("midi-forge mcp is handled at process entry (stdio MCP server)");
            print_help();
            2
        }
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
  midi-forge send --out <name> note <note> <vel> [--ch N] [--m2] [--group G]
  midi-forge send --out <name> cc <cc> <val> [--ch N] [--m2] [--group G]
  midi-forge send --out <name> rc --bank B --index I --val V [--ch N] [--group G]
  midi-forge identity --out <name>
  midi-forge panic --out <name>
  midi-forge receive --in <name> [--seconds N]
  midi-forge clock --out <name> [--bpm 120] [--seconds 2]
  midi-forge mcp [--attach] [--standalone] [--arm] [--mcp-port 7420] [--mcp-url http://127.0.0.1:7420/mcp]
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

const VALUE_FLAGS: &[&str] = &["--out", "--ch", "--group", "--bank", "--index", "--val"];

fn parse_u32_auto(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse().ok()
    }
}

fn send_kind(args: &[String]) -> Option<&str> {
    args.iter()
        .skip(2)
        .map(String::as_str)
        .find(|a| matches!(*a, "note" | "cc" | "rc"))
}

fn positional_send(args: &[String]) -> Vec<&str> {
    args.iter()
        .skip(2)
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .filter(|a| !matches!(*a, "note" | "cc" | "rc" | "send"))
        .filter(|a| !VALUE_FLAGS.iter().any(|f| flag(args, f) == Some(a)))
        .collect()
}

fn build_send_packet(args: &[String]) -> Result<UmpMessage, String> {
    let ch = flag(args, "--ch")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1)
        .saturating_sub(1)
        .min(15);
    let group = flag(args, "--group")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(0)
        .min(15);
    let m2 = has(args, "--m2");
    let positional = positional_send(args);
    match send_kind(args) {
        Some("note") => {
            let note: u8 = positional
                .first()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);
            let vel: u8 = positional
                .get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);
            if m2 {
                Ok(midi2_note_on(group, ch, note, velocity7_to_16(vel)))
            } else {
                Ok(UmpMessage::midi1_channel_voice(group, 0x90 | ch, note, vel))
            }
        }
        Some("cc") => {
            let cc: u8 = positional.first().and_then(|s| s.parse().ok()).unwrap_or(1);
            let val: u8 = positional
                .get(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(127);
            if m2 {
                Ok(midi2_cc(group, ch, cc, value7_to_32(val)))
            } else {
                Ok(UmpMessage::midi1_channel_voice(group, 0xB0 | ch, cc, val))
            }
        }
        Some("rc") => {
            let bank: u8 = flag(args, "--bank")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let index: u8 = flag(args, "--index")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let val = flag(args, "--val")
                .and_then(parse_u32_auto)
                .unwrap_or(0x8000_0000);
            Ok(midi2_registered_controller(group, ch, bank, index, val))
        }
        _ => Err("send note|cc|rc …".into()),
    }
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
    let pkt = match build_send_packet(args) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("{err}");
            return 2;
        }
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
    let result = backend.send(&ep.id, &pkt);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        std::iter::once("midi-forge")
            .chain(parts.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn m2_note_uses_group() {
        let a = args(&[
            "send", "--out", "x", "--m2", "--group", "2", "note", "60", "100",
        ]);
        let pkt = build_send_packet(&a).unwrap();
        assert_eq!(pkt.message_type(), 4);
        assert_eq!(pkt.group(), 2);
        assert_eq!(pkt.data1(), 60);
        assert_eq!(pkt.status_byte() & 0x0F, 0);
        assert_eq!(pkt.words()[1] >> 16, u32::from(velocity7_to_16(100)));
    }

    #[test]
    fn rc_parses_hex_val() {
        let a = args(&[
            "send",
            "--out",
            "x",
            "rc",
            "--bank",
            "0",
            "--index",
            "6",
            "--val",
            "0x80000000",
        ]);
        let pkt = build_send_packet(&a).unwrap();
        assert_eq!(pkt.message_type(), 4);
        assert_eq!(pkt.status_byte() & 0xF0, 0x20);
        assert_eq!(pkt.data1(), 0);
        assert_eq!(pkt.data2(), 6);
        assert_eq!(pkt.words()[1], 0x8000_0000);
    }

    #[test]
    fn midi1_note_keeps_group() {
        let a = args(&["send", "--out", "x", "--group", "3", "note", "48", "10"]);
        let pkt = build_send_packet(&a).unwrap();
        assert_eq!(pkt.message_type(), 2);
        assert_eq!(pkt.group(), 3);
        assert_eq!(pkt.data1(), 48);
        assert_eq!(pkt.data2(), 10);
    }

    #[test]
    fn dispatch_includes_mcp() {
        assert!(dispatch(&args(&["mcp"])));
        assert!(dispatch(&args(&["mcp", "--attach", "--arm"])));
    }
}
