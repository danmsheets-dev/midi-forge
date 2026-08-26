# Phase 3 Data Maps + Patchbay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Per-connection MIDI-OX-style data maps (match → drop or rewrite type/channel/data), a visual cable patchbay, and JSON profile save/load.

**Architecture:** `DataMap` runs after `Filter` on each `Link`. Profiles serialize endpoint ids plus filter+map, not session `PortId`s. The patchbay is a painter view of the same `Router` graph as the matrix.

**Tech Stack:** serde / serde_json in core; rfd file dialogs in the app.

**Spec:** `docs/superpowers/specs/2026-08-26-midi-forge-architecture.md`

---

## Pipeline

```
incoming → Filter.apply → DataMap.apply → send
```

First matching map entry wins. Channel-voice only; clock/SysEx pass the map unchanged (still subject to the filter).

## File map

| Path | Responsibility |
|------|----------------|
| `crates/midi-forge-core/src/map.rs` | Matcher, ValueMap, DataMap |
| `crates/midi-forge-core/src/profile.rs` | JSON profile |
| `crates/midi-forge-core/src/router.rs` | `Link.map` |
| `crates/midi-forge-app/src/thru.rs` | Matrix, patchbay, filter, maps |
| `crates/midi-forge-app/src/app.rs` | Save/load, route |

---

### Task 1: DataMap

- [x] Empty map is identity
- [x] Transpose notes by offset, clamp 0–127
- [x] Remap CC number
- [x] Drop matching messages
- [x] Convert CC → NoteOn
- [x] Invert velocity
- [x] First match wins; unmatched pass or drop

### Task 2: Router + profile

- [x] Map applied after filter
- [x] JSON round-trip of links by endpoint id

### Task 3: UI

- [x] Patchbay cables
- [x] Map table + presets on selected link
- [x] Save / Load profile
