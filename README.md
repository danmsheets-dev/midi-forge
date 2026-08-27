# Midi-Forge 0.1 Beta

A 64-bit MIDI diagnostic and routing utility — a MIDI-OX successor for the MIDI 2.0 era.

Rust engine, native desktop (Windows first, macOS CoreMIDI). Capture, thru, maps, SysEx, MPE, Live, clock, Lua, CLI, and a local MCP port so an AI technician can help without sitting on the MIDI callback.

This is a **beta**. It is meant for the bench: keyboards, modules, vintage UARTs, USB MIDI 2.0, and DAW cables. It is not a DAW.

## Get the app (Windows)

Download **`midi-forge.exe`** from the [latest GitHub Release](https://github.com/danmsheets-dev/midi-forge/releases).

No installer. Double-click to open the GUI. From a terminal:

```text
midi-forge.exe --list
midi-forge.exe help
```

The GUI auto-opens every MIDI input. Play a connected keyboard — notes should appear in the monitor.

Optional native MIDI 2.0 on Windows (Windows MIDI Services):

```text
winget install Microsoft.WindowsMIDIServicesSDK
```

With the App SDK registered, Midi-Forge uses a live `MidiSession` (UMP on the wire). Without it, WinMM stays the backend. It never fakes a UMP session.

## What 0.1 Beta can do

| Area | In this release |
|------|-----------------|
| **Monitor** | UMP-canonical log, named decode, UMP words column, type/channel filters, search, copy, export |
| **Thru / patchbay** | Matrix + cables, per-link filter and data map, learn, scenes in a JSON profile |
| **Live / MPE** | Sounding notes, last CC, bend; MPE zones (RPN 6) and voices; stuck-note list |
| **Inject** | On-screen keyboard + CC to a chosen output; MIDI 2 group / 16-bit velocity / attributes |
| **Clock** | Host BPM, jitter, SPP, MTC; clock master (start/stop/continue) from the engine thread |
| **SysEx** | Armed receive, dump wizard, handshake, hex diff, gap after F7, GM/GS/XG packs |
| **Lua** | Sandboxed 5.4 `on_midi` before thru; timers and `midi.state` in the profile; no `io` / `os` |
| **CLI** | `send` / `receive` / `identity` / `panic` / `clock` / `--list` |
| **MIDI 2 language** | Every defined UMP type named and constructed: MIDI 2 CV, Utility/JR, SysEx8, Flex, Stream |
| **I/O** | Per-endpoint downscale (MIDI 1 dests only). WMS `MidiSession`, WinMM fallback, CoreMIDI, in-app loopbacks |
| **MCP** | Local technician copilot — see below |

Panic sends All Sound Off / Reset CC / All Notes Off on all 16 channels to every output it can open.

## Agent / MCP (AI technician)

The running GUI does **not** expose MCP until you tick **Agent** in the banner. That serves tools on `http://127.0.0.1:7420/mcp` only (never `0.0.0.0`).

**Arm writes** is a second checkbox, off by default. Without it the agent can only read.

Same binary from an IDE:

```text
midi-forge mcp
```

That stdio server probes the GUI. If Agent is on, it **attaches** to the live session (does not open a second WinMM stack). If the GUI is down it starts a standalone MIDI session. `--standalone` skips the probe. `--arm` applies only to standalone; when attached, GUI **Arm writes** is the source of truth.

Cursor / Claude Code / Grok:

```json
{
  "mcpServers": {
    "midi-forge": {
      "command": "C:\\\\path\\\\to\\\\midi-forge.exe",
      "args": ["mcp"]
    }
  }
}
```

Tools: `list_endpoints`, `monitor_tail`, `live_now`, `clock_health`, `stuck_notes`, `thru_graph`, `mpe_status`, `snapshot`, `send_note`, `send_cc`, `identity`, `panic`, `set_port_open`.

v1 does **not** expose SysEx librarian or MIDI-CI PE SET. Do not exclusive-open the same WinMM input from two processes — use attach.

## MIDI 2.0 notes

- Monitor names MIDI 2 channel voice (per-note, RC/AC, attributes), Utility, Flex Data, Stream, SysEx8 / MixData.
- Inject **MIDI 2** sends 16-bit velocity / 32-bit CC (group + attribute type). Maps can use **C32**.
- Downscale is **per-endpoint**, not backend-wide: WinMM / GS Wavetable project type `0x4` → `0x2`; UMP dests (`wms:…`, `forge:loop:*`, CoreMIDI MIDI 2) pass type `0x4` unchanged.
- **Add DAW loop** still shells `midi.exe` when MidiSrv and SDK Tools are present.

## Not in 0.1 (honest leftover)

- Scheduled / timestamped WMS send (`send_at`, JR schedule)
- Full MIDI-CI Property Exchange JSON session
- MIDI Clip File / SMF2 (SMF0 record/play is in)
- Network MIDI 2.0 session/auth (UDP invitation + datagrams exist)
- SysEx / PE SET over MCP

## Build from source

Requires stable Rust (`rust-toolchain.toml`).

```text
cargo test --workspace
cargo run -p midi-forge-app -- --list
cargo run -p midi-forge-app
```

CLI examples:

```text
cargo run -p midi-forge-app -- send --out "GS Wavetable" note 60 100
cargo run -p midi-forge-app -- receive --in "MPK" --seconds 3
cargo run -p midi-forge-app -- clock --out "GS Wavetable" --bpm 120 --seconds 2
cargo run -p midi-forge-app -- mcp --standalone
```

Release binary:

```text
cargo build --release -p midi-forge-app
```

The GUI binary is `#![windows_subsystem = "windows"]`. `--list` and CLI modes attach the parent console.

## Crates

| Crate | Role |
|-------|------|
| `midi-forge-core` | UMP types, MIDI 1 parser, decode. No OS deps. |
| `midi-forge-io` | `MidiBackend`: WMS, WinMM, CoreMIDI, loopback, null |
| `midi-forge-script` | Sandboxed Lua 5.4 |
| `midi-forge-app` | egui desktop shell (`midi-forge` binary) |

Capture, thru, Lua, and clock master run on a dedicated **midi-engine** thread (1 ms tick). The UI locks the engine only while drawing.

Architecture: `docs/superpowers/specs/2026-08-26-midi-forge-architecture.md`

## License

MIT. See `LICENSE-MIT`.
