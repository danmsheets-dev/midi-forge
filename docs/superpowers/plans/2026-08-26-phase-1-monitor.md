# Phase 1 Live Monitor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Open WinMM MIDI inputs, parse complete short messages and SysEx into UMP on a non-callback thread, show a scrolling timestamped monitor, and send a panic (CC 120/121/123 × 16 channels) to open outputs.

**Architecture:** WinMM `CALLBACK_FUNCTION` only `try_send`s a `Copy` capture frame into a bounded `sync_channel`. The UI thread `poll`s, converts to `MidiEvent`, and appends to `MonitorLog`. Callbacks never parse, allocate, or block.

**Tech Stack:** Existing crates, `std::sync::mpsc::sync_channel`, WinMM `midiInOpen` / `midiOutShortMsg`.

**Spec:** `docs/superpowers/specs/2026-08-26-midi-forge-architecture.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/midi-forge-core/src/midi1.rs` | Packed WinMM DWORD → UMP; UMP → packed short |
| `crates/midi-forge-core/src/panic.rs` | All-sound-off / reset-cc / all-notes-off |
| `crates/midi-forge-core/src/log.rs` | Bounded monitor ring |
| `crates/midi-forge-io/src/backend.rs` | `open_input` / `open_output` / `poll` / `send` |
| `crates/midi-forge-io/src/winmm.rs` | Callback, SysEx headers, poll convert |
| `crates/midi-forge-io/src/null.rs` | Inject + record sends for tests |
| `crates/midi-forge-app/src/app.rs` | Open checkboxes, log table, pause/clear/panic |

---

### Task 1: Packed short ↔ UMP

WinMM `MIM_DATA` `dwParam1` is `status | data1<<8 | data2<<16`.

- [x] `ump_from_packed_short(0x007F_3C90)` → `0x2090_3C7F`
- [x] Clock `0x0000_00F8` → type 1 system
- [x] Round-trip `packed_short_from_ump`

### Task 2: Panic packets + monitor log

- [x] `panic_packets()` length 48, channels 0–15, CC 120 then 121 then 123
- [x] `MonitorLog` capacity 2 evicts oldest

### Task 3: Backend trait + NullBackend

- [x] `open_input` / `poll` / `send` on NullBackend
- [x] Injected note-on appears on poll

### Task 4: WinMM streams

- [x] `midiInOpen` + `CALLBACK_FUNCTION` + `try_send`
- [x] 8×1024 SysEx headers re-queued on `MIM_LONGDATA`
- [x] `midiOutShortMsg` for send
- [x] Drop closes handles

### Task 5: App

- [x] Auto-open all inputs
- [x] Virtualized log: time, port, hex, decoded
- [x] Pause, Clear, Panic, follow-tail, dropped count
- [x] Repaint while capturing
