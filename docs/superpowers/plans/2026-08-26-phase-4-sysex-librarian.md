# Phase 4 SysEx Librarian Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Capture, edit, load/save, and send System Exclusive dumps the way MIDI-OX’s SysEx window did: identity request, manual receive, `.syx` files, hex view, delay-after-F7.

**Architecture:** Core owns dump bytes and UMP reassembly. IO sends a complete `F0…F7` buffer via `midiOutLongMsg`. The UI never sleeps; multi-dump send is a frame-driven job with `delay_ms` after each F7.

**Tech Stack:** Existing crates. No new MIDI APIs beyond WinMM long messages.

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/midi-forge-core/src/sysex.rs` | Dump, assembler, `.syx`/hex, identity, Roland checksum |
| `crates/midi-forge-io/src/backend.rs` | `send_sysex` |
| `crates/midi-forge-io/src/winmm.rs` | `midiOutLongMsg` |
| `crates/midi-forge-app/src/sysex.rs` | Librarian panel |

---

### Task 1: Core dumps

- [x] Identity request bytes
- [x] Assemble complete / start-end UMP SysEx7
- [x] Parse and emit `.syx` (multiple dumps)
- [x] Parse hex text
- [x] Roland checksum of payload excluding last byte
- [x] Identity reply decode

### Task 2: IO

- [x] `MidiBackend::send_sysex`
- [x] NullBackend records bytes
- [x] WinMM prepare / long message / wait MHDR_DONE

### Task 3: App

- [x] Right-hand librarian: arm receive, identity, delay, hex, load/save/send
- [x] Frame-driven delay-after-F7
- [x] Thru of SysEx reassembles then `send_sysex`
