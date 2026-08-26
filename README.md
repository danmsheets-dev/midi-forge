# Midi-Forge

**0.1 Beta** — modern 64-bit MIDI diagnostic and routing utility (MIDI-OX successor). Rust engine, native Windows/macOS desktop first.

Lua 5.4 can drop, rewrite, or fan out captured events before thru. The monitor still shows the raw wire. Scripts save with the JSON profile.

## Build

Requires stable Rust (see `rust-toolchain.toml`).

```text
cargo test --workspace
cargo run -p midi-forge-app -- --list
cargo run -p midi-forge-app
```

`--list` prints endpoints with a MIDI 1 / UMP protocol tag. The GUI auto-opens every input. Play notes on a connected keyboard and they should appear in the monitor. MIDI 2 packets show as `M2 NoteOn` (16-bit velocity) and similar.

Thru: tick a cell in the bottom matrix (for example MPK mini play → Microsoft GS Wavetable Synth). Cables on the patchbay follow the same graph. Select a cell to edit its filter and data map (transpose, CC remap, invert velocity, type conversion). **Save** / **Load** write a JSON profile.

Uncheck **Clock** to strip MIDI clock. Pause freezes the log but does not stop thru.

SysEx: arm receive, then dump from hardware. **Dump wizard** retries identity and names the maker (Roland, Yamaha, …). **Handshake** waits for an F7 before the next dump. **Hex diff** compares two captured dumps. **Thru gap** spaces short messages for vintage UARTs. **Gap after F7** still applies between dumps.

MPE: the monitor shows zones (RPN 6) and live notes with bend/pressure/timbre (CC74). **Add cable** creates an in-app loopback on Windows (`forge:loop:*`, not visible to other programs) or a CoreMIDI virtual pair on macOS (other apps can see it). loopMIDI / MIDI Services ports still appear in the Windows endpoint list. When MidiSrv is running the banner shows **MidiSrv**.

Lua: the right panel **Lua** tab. **Apply** compiles, **Enable** runs `on_midi` on capture before thru. `print` / `midi.log` go to the script log. `io` / `os` are not available.

**Inject:** on-screen two-octave keyboard and CC slider send to a chosen output. **Monitor** can filter by type/channel, search, copy, and export. **Mute clock** strips clock on thru only. Green dots on endpoints flash with traffic. Stuck notes list under MPE; Panic also sends note-offs for hanging notes. Devices rescanning every 2s when the list changes.

**Live** is a ShowMIDI-style “now” view (notes sounding, last CC, bend per channel) above the monitor. Named CCs appear in the log (`CC7 (Volume)`); RPN/NRPN assemble from CC 98–101. **MIDI-CI** next to Identity sends a Discovery inquiry. Endpoints show whether MidiSrv / `midi.exe` are present — native UMP `MidiSession` still needs the Windows MIDI Services App SDK.

Panic sends All Sound Off / Reset CC / All Notes Off on all 16 channels to every output it can open.

## Crates

| Crate | Role |
|-------|------|
| `midi-forge-core` | UMP types, MIDI 1.0 parser, decode. No OS deps. |
| `midi-forge-io` | `MidiBackend` trait, NullBackend, WinMM enumerate |
| `midi-forge-script` | Sandboxed Lua 5.4 (`on_midi`) |
| `midi-forge-app` | egui desktop shell (`midi-forge` binary) |

Architecture: `docs/superpowers/specs/2026-08-26-midi-forge-architecture.md`  
Phase 0 plan: `docs/superpowers/plans/2026-08-26-phase-0-workspace.md`
