# Midi-Forge

Modern 64-bit MIDI diagnostic and routing utility (MIDI-OX successor). Rust engine, native Windows/macOS desktop first.

Phase 7 adds a sandboxed Lua 5.4 processor (`on_midi`) that can drop, rewrite, or fan out captured events before thru. The monitor still shows the raw wire. Scripts save with the JSON profile.

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

SysEx: arm receive on the right panel, or send **Identity request** to the selected output. **Delay after F7** (default 60 ms) spaces dumps when sending a `.syx` file.

MPE: the monitor shows zones (RPN 6) and live notes with bend/pressure/timbre (CC74). **Add cable** creates an in-app loopback on Windows (`forge:loop:*`, not visible to other programs) or a CoreMIDI virtual pair on macOS (other apps can see it). loopMIDI / MIDI Services ports still appear in the Windows endpoint list. When MidiSrv is running the banner shows **MidiSrv**.

Lua: the right panel **Lua** tab. **Apply** compiles, **Enable** runs `on_midi` on capture before thru. `print` / `midi.log` go to the script log. `io` / `os` are not available.

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
