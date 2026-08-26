# Phase 6 UMP + CoreMIDI + Windows MIDI Services Probe

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Treat UMP as a first-class wire format in the monitor (MIDI 2.0 channel voice decode and downscale to MIDI 1 for WinMM). On macOS, talk CoreMIDI including virtual ports. On Windows, detect Midisrv and keep WinMM I/O (the service already translates UMP devices for WinMM). Full WinRT MidiSession I/O waits on the App SDK projection (not on crates.io).

**Architecture:** Core gains type-0x4 decode/`downscale_to_midi1`. WinMM `send` downscales. `CoreMidiBackend` is `cfg(macos)`. Windows probes the `MidiSrv` service and surfaces that in the UI.

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/midi-forge-core/src/midi2.rs` | MIDI 2 CV decode + downscale |
| `crates/midi-forge-core/src/decode.rs` | Type 0x4 in `decode()` |
| `crates/midi-forge-io/src/winmm.rs` | Downscale on send; Midisrv probe |
| `crates/midi-forge-io/src/coremidi.rs` | macOS I/O + virtual endpoints |
| Endpoint list | Protocol hint |

---

### Task 1: MIDI 2.0

- [x] Note on/off 16-bit velocity decode
- [x] Downscale velocity to 7-bit for WinMM
- [x] CC 32-bit → 7-bit

### Task 2: Backends

- [x] WinMM send uses downscale
- [x] MidiSrv running → backend name `winmm+midisrv`
- [x] CoreMIDI module behind `cfg(target_os = "macos")` (`coremidi` crate 0.9, virtual source/dest)

### Task 3: UI

- [x] Show Midi1 vs UMP on endpoints
- [x] MIDI 2 summaries in the monitor
- [x] MidiSrv badge when Windows MIDI Services is running
