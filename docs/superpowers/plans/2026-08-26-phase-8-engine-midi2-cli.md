# Phase 8 — Engine thread, MIDI 2 language, clock master, CLI, WMS caps

> **For agentic workers:** Implement in this workspace. TDD for core modules (`clock_master`, MIDI 2 decode, session ingest). GUI wiring follows once core tests pass.

**Goal:** Take MIDI processing off the UI frame, speak MIDI 2.0 channel-voice (including per-note and registered/assignable controllers), generate clock, and add a headless CLI — while leaving a clean socket for native `MidiSession`.

**Architecture:** `midi-forge-core` gains `ClockMaster`, richer MIDI 2 constructors/decode, and 32-bit map values. `midi-forge-io` reports `BackendCaps` and tags software loopbacks as UMP (pass-through). `midi-forge-app` runs a `midi-engine` thread that owns the `MidiBackend`; the UI copies a snapshot each frame and sends commands for inject/thru/panic. CLI is the same binary with argv other than the GUI.

**Tech Stack:** Rust 2024, existing crates, no WinRT SDK, no new GUI toolkit.

---

## File map

| Path | Role |
|------|------|
| `crates/midi-forge-core/src/clock_master.rs` | Generate F8/FA/FB/FC/F2 |
| `crates/midi-forge-core/src/midi2.rs` | Per-note + RC/AC construct/downscale |
| `crates/midi-forge-core/src/decode.rs` | Named MIDI 2 summaries |
| `crates/midi-forge-core/src/map.rs` | `ValueMap::Constant32` / `Scale32` |
| `crates/midi-forge-core/src/filter.rs` | `per_note` kind |
| `crates/midi-forge-core/src/ump.rs` | Helper constructors |
| `crates/midi-forge-io/src/backend.rs` | `BackendCaps` |
| `crates/midi-forge-io/src/loopback.rs` | `ProtocolHint::Ump` |
| `crates/midi-forge-script/src/engine.rs` | `Send` (Arc log) |
| `crates/midi-forge-app/src/engine.rs` | Engine thread + commands |
| `crates/midi-forge-app/src/cli.rs` | Headless commands |
| `crates/midi-forge-app/src/clock.rs` | Master controls |
| `crates/midi-forge-app/src/inject.rs` | MIDI 2 velocity |
| `crates/midi-forge-app/src/main.rs` | Dispatch CLI vs GUI |
| `README.md` | Phase 8 usage |

### Task 1 — Clock master (core)

- [x] Tests: 120 BPM interval, start emits FA then F8s, stop emits FC, catch-up cap
- [x] `ClockMaster::poll(now_ns) -> Vec<UmpMessage>`

### Task 2 — MIDI 2 language (core)

- [x] Decode per-note pitch bend, per-note CC, registered/assignable controllers, per-note management
- [x] Downscale RC/AC to RPN/NRPN; drop per-note on MIDI 1 outputs
- [x] `ValueMap` 32-bit variants; MIDI 2 rewrite uses them

### Task 3 — Backend caps + UMP loopback

- [x] `MidiBackend::caps()`
- [x] Loopback `ProtocolHint::Ump` (no downscale)

### Task 4 — Engine thread

- [x] Backend poll/send/clock master on `midi-engine` thread
- [x] UI does not call `drain_capture` on the frame
- [x] Lua/router still run; snapshot for monitor

### Task 5 — CLI

- [x] `midi-forge send|receive|identity|panic|clock|--list|--help`

### Task 6 — UI

- [x] Clock master enable/BPM/start/stop/continue/destination
- [x] Inject MIDI 2 checkbox
- [x] Banner shows `native UMP` vs `MIDI 1 wire`
