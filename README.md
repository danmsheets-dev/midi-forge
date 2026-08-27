# Midi-Forge

**0.1 Beta** — modern 64-bit MIDI diagnostic and routing utility (MIDI-OX successor). Rust engine, native Windows/macOS desktop first.

Lua 5.4 can drop, rewrite, or fan out captured events before thru. The monitor still shows the raw wire. Scripts save with the JSON profile.

## Build

Requires stable Rust (see `rust-toolchain.toml`).

```text
cargo test --workspace
cargo run -p midi-forge-app -- --list
cargo run -p midi-forge-app -- help
cargo run -p midi-forge-app -- send --out "GS Wavetable" note 60 100
cargo run -p midi-forge-app -- receive --in "MPK" --seconds 3
cargo run -p midi-forge-app -- clock --out "GS Wavetable" --bpm 120 --seconds 2
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

**Clock** shows host-receive BPM, jitter, runaway clock, song position, and MTC. **Master** (Enable + BPM + Start/Stop/Continue) generates F8/FA/FB/FC on a chosen output from the `midi-engine` thread. Histograms are USB/driver timing, not 5-pin delay. **Thru path** lists which outputs a note actually hit. **Snap** copies live + clock + stuck notes + recent thru. **Learn** on a thru map waits for the next CC/note. Exclusive-open errors name likely DAWs from window titles. **On top** + a larger **PANIC** are for the live bench.

**MIDI 2:** the monitor names per-note bend/controllers and registered/assignable controllers (`M2 RC`, `M2 PN Bend`). Inject **MIDI 2** sends 16-bit velocity / 32-bit CC. Thru maps can use **C32** (32-bit constant). WinMM still downscales to MIDI 1; in-app loopbacks (`forge:loop:*`) pass UMP unchanged. Native `MidiSession` I/O is the next MIDI 2 I/O phase — see `docs/superpowers/specs/2026-08-26-midi2-roadmap.md`.

**Record / Play SMF** on the monitor toolbar writes format-0 `.mid` files. **PE GET/SET** and a **Device** library live on the SysEx tab. Lua: `midi.after(ms, ev)`, `midi.state` (saved in the profile), optional `on_idle`. **Net** tab: UDP Network MIDI 2.0 invitation + UMP datagrams (port 5004).

Capture, thru, Lua, and clock master run on a dedicated **midi-engine** thread (1 ms tick). The UI locks the engine only while drawing.

**Scenes** (name + Save scene) store thru, Lua, mute clock, and throttle in the JSON profile. SysEx **pack** sends GM/GS/XG/Sequential/Korg/Yamaha dump requests. **CI Profiles** / **CI PE** are MIDI-CI inquiries (not full property-exchange JSON). MPE shows whether a zone is actually configured. **Add DAW loop** runs `midi loopback create` when MidiSrv and SDK Tools are present — still not a native `MidiSession`.

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
