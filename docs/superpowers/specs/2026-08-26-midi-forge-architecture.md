# Midi-Forge Architecture

Date: 2026-08-26  
Status: accepted  
Product: Midi-Forge — modern 64-bit MIDI-OX replacement  
v1 target: Windows + macOS desktop, **full MIDI-OX parity plus MPE and MIDI 2.0 UMP**

## Goal

Ship a native desktop MIDI diagnostic/router that a studio technician can leave running all day: live monitor, N×M thru, filters, data maps, SysEx librarian, virtual ports, MPE inspector, and a MIDI 2.0 UMP view. The engine is Rust. The first UI is Windows (user has a USB MIDI keyboard attached now). macOS uses the same crates. iOS/Android are later shells over `midi-forge-core` + `midi-forge-io`, not a port of the desktop layout.

## Crate map

```
midi-forge/                  workspace
  crates/midi-forge-core     protocol, parse, filter graph, maps, log model
                             ZERO OS / GUI dependencies
  crates/midi-forge-io       backends: WinMM, Windows MIDI Services, CoreMIDI, …
                             depends on core only
  crates/midi-forge-app      eframe/egui desktop shell
                             depends on core + io
```

Rules:

- `core` must compile with `cargo test -p midi-forge-core` on any host, no MIDI hardware.
- All protocol tests use byte/UMP fixtures, never live devices.
- The GUI never talks to WinMM/CoreMIDI directly.
- MIDI callbacks never block on UI, disk, or allocation-heavy decode. Capture into a bounded lock-free queue; the UI and filter graph consume on their own threads.

Future crates (not in Phase 0): `midi-forge-script` (Lua), `midi-forge-ffi` (mobile).

## Canonical event model

Every event inside the engine is a **Universal MIDI Packet**, even when the wire was MIDI 1.0.

```
OS bytes or UMP  →  Midi1Parser / UmpFramer  →  UmpMessage  →  MidiEvent
                                                              (timestamp + port + packet)
```

- MIDI 1.0 channel/system messages become UMP message type `0x2` / `0x1`.
- SysEx becomes UMP SysEx7 (`0x3`) chunks.
- MIDI 2.0 devices produce type `0x4` channel voice and other UMP types natively.
- The monitor can *project* a packet as MIDI 1.0 hex, decoded English, or raw UMP words.

This is the Option C decision: we do not have a MIDI 1.0-only engine with MIDI 2.0 bolted on later.

## Threading

1. **Backend thread(s)** — OS callback or waitable queue. Push `MidiEvent` into a bounded SPSC/MPSC ring (`thingbuf` or equivalent). Drop policy: never block the callback; increment a dropped-count visible in the UI.
2. **Engine thread** — pop events, run filter/map/thru, write to outputs, append to the monitor log (ring of N events).
3. **UI thread** — egui. Reads a snapshot of the log and endpoint list. Sends commands (open port, panic, load map) through a command channel.

Phase 0 does not start the engine thread. It locks the types and crate boundaries so Phase 1 can.

## Desktop I/O strategy (Windows)

| OS | MIDI 1.0 | MIDI 2.0 / virtual ports |
|----|----------|---------------------------|
| Windows 10 | WinMM | not available; virtual ports via third-party loopback later |
| Windows 11 | WinMM fallback **and** Windows MIDI Services | UMP, loopback, virtual device app |
| macOS | CoreMIDI | CoreMIDI UMP + virtual endpoints / IAC |

Phase 0 implements **WinMM enumeration only** so a connected USB keyboard is visible. Opening streams is Phase 1.

Do not make `midir` the architecture. It has no MIDI 2.0 and no Windows virtual ports.

## v1 feature set (Option C)

MIDI-OX parity: live monitor, port matrix thru, filters, data maps, SysEx librarian (delay-after-F7, dumps, identity), on-screen keyboard, saveable profiles.

Plus: MPE zone view, UMP monitor, 1.0↔2.0 display projection, Win11 virtual/loopback ports, panic, hotplug.

Out of v1: Network MIDI 2.0, BLE, Lua, mobile shells, plugin hosting.

## GUI

egui / eframe. Immediate-mode fits a live log. First window: endpoint list + empty monitor pane + build/phase badge. Visual mapper and docks come after the engine is live.

## Testing

- Unit: UMP framing, MIDI 1.0 state machine (running status, realtime-in-sysex, chunked SysEx7).
- Golden fixtures under `crates/midi-forge-core/tests/fixtures/`.
- IO: `NullBackend` always. WinMM enumerate is manual (`midi-forge --list`) plus an ignored integration test.
- No UI screenshot tests in Phase 0.

## Phases after 0

1. Open WinMM input, timestamped monitor, panic.
2. Thru + filters.
3. Data maps + visual patchbay.
4. SysEx librarian.
5. Virtual ports + MPE.
6. Windows MIDI Services UMP + CoreMIDI.
7. Lua.
8. Mobile FFI.

## Key decisions

1. **Rust + egui**, not JUCE — safety, license, tool UI; mobile is a later shell.
2. **UMP-canonical core** from Phase 0 — Option C.
3. **Three crates**, OS behind `MidiBackend`.
4. **WinMM first on Windows**, MIDI Services as a second backend, not a rewrite.
5. **v1 is desktop parity + MPE + UMP**; scripting and mobile wait.
