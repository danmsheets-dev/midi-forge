# Phase 2 Thru + Filter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Route captured MIDI from inputs to outputs through a port matrix, with per-connection filters (message class, channel mask, channel remap). Thru keeps running when the monitor is paused.

**Architecture:** Pure `Filter` + `Router` in `midi-forge-core` (PortId → PortId). The app maps PortIds to WinMM endpoints and `send`s after `poll`. No new OS APIs.

**Tech Stack:** Existing crates.

**Spec:** `docs/superpowers/specs/2026-08-26-midi-forge-architecture.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/midi-forge-core/src/ump.rs` | `channel` / `with_channel` |
| `crates/midi-forge-core/src/filter.rs` | Pass/drop + remap |
| `crates/midi-forge-core/src/router.rs` | N×M connections |
| `crates/midi-forge-app/src/app.rs` | Matrix UI, thru on poll |

---

### Task 1: Packet channel helpers

- [x] `channel()` on MIDI 1.0 channel voice
- [x] `with_channel(3)` rewrites status nibble

### Task 2: Filter

- [x] Default filter passes note-on and clock
- [x] `clock: false` drops F8, keeps notes
- [x] Channel mask drops other channels
- [x] `force_channel` remaps after the mask

### Task 3: Router

- [x] Fan-out to two outputs
- [x] Disabled/missing link emits nothing

### Task 4: App

- [x] Stable PortId per endpoint
- [x] Bottom matrix: input rows × output columns
- [x] Enabling a cell opens the output and stores a default filter
- [x] Selected cell edits that filter
- [x] `poll` → log (if not paused) → `router.route` → `send`
- [x] Thru continues while Pause is on
