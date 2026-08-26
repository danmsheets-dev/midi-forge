# Phase 5 MPE + Virtual Cables Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** An MPE zone/voice inspector driven by RPN 6 (MCM) and per-note X/Y/Z, plus app-local virtual loopback cables that show up as endpoints. DAW-visible ports remain whatever WinMM already enumerates (loopMIDI, MIDI Services basic loopbacks).

**Architecture:** `MpeTracker` in core (no OS). `SoftwareLoopbacks` in io, owned by WinMM and Null backends. Sending to `forge:loop:N:out` queues events on `forge:loop:N:in`.

**Tech Stack:** Existing crates. No Windows MIDI Services App SDK in this phase (Phase 6).

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/midi-forge-core/src/mpe.rs` | Zones, RPN, voices |
| `crates/midi-forge-io/src/loopback.rs` | In-process A/B cables |
| `crates/midi-forge-app/src/mpe.rs` | Inspector + MCM send |
| Left endpoint panel | Add / remove Forge cable |

---

### Task 1: MPE

- [x] MCM on ch1 sets lower-zone member count
- [x] MCM 0 disables the zone
- [x] Note on/off, pitch bend, CC74, channel pressure on a member
- [x] RPN 0 sets pitch-bend range
- [x] Helper packets to send MCM

### Task 2: Loopbacks

- [x] create pair, send to out, poll on in
- [x] WinMM refresh keeps Forge cables

### Task 3: UI

- [x] MPE table in the monitor
- [x] Add Forge cable button
