# Phase 0 Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Midi-Forge Cargo workspace so `core` has a tested UMP-canonical MIDI 1.0 parser, `io` can list Windows WinMM endpoints, and `app` is a native egui window that shows those endpoints.

**Architecture:** Three crates (`midi-forge-core`, `midi-forge-io`, `midi-forge-app`). Core has zero OS deps. All live MIDI enters as `UmpMessage`. WinMM is linked via `winmm` FFI for enumeration only.

**Tech Stack:** Rust 1.97 (edition 2024), eframe 0.36, thiserror 2, WinMM on Windows.

**Spec:** `docs/superpowers/specs/2026-08-26-midi-forge-architecture.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `Cargo.toml` | Workspace, shared deps |
| `rust-toolchain.toml` | Pin stable + rustfmt/clippy |
| `.gitignore` | `target/`, editor junk |
| `crates/midi-forge-core/src/ump.rs` | `UmpMessage` newtype |
| `crates/midi-forge-core/src/midi1.rs` | Byte stream → UMP |
| `crates/midi-forge-core/src/event.rs` | `MidiEvent`, `PortId`, `Timestamp` |
| `crates/midi-forge-core/src/decode.rs` | Monitor-facing decode |
| `crates/midi-forge-core/src/error.rs` | `CoreError` |
| `crates/midi-forge-io/src/backend.rs` | `MidiBackend` trait |
| `crates/midi-forge-io/src/null.rs` | Test backend |
| `crates/midi-forge-io/src/winmm.rs` | WinMM enumerate |
| `crates/midi-forge-app/src/main.rs` | `--list` and eframe entry |
| `crates/midi-forge-app/src/app.rs` | Endpoint list UI |

---

### Task 1: Workspace skeleton

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `.gitignore`, `LICENSE-MIT`
- Create: `crates/midi-forge-core/Cargo.toml`, `crates/midi-forge-core/src/lib.rs`
- Create: `crates/midi-forge-io/Cargo.toml`, `crates/midi-forge-io/src/lib.rs`
- Create: `crates/midi-forge-app/Cargo.toml`, `crates/midi-forge-app/src/main.rs`

- [x] **Step 1: Write workspace manifests and empty crates**
- [x] **Step 2: `cargo test --workspace` compiles (core/io tests empty; app is a binary)**

---

### Task 2: `UmpMessage`

**Files:**
- Create: `crates/midi-forge-core/src/ump.rs`
- Create: `crates/midi-forge-core/src/error.rs`
- Modify: `crates/midi-forge-core/src/lib.rs`

- [x] **Step 1: Write failing tests** for word count, MIDI 1 note-on packing, reject short type-4 packet
- [x] **Step 2: Run `cargo test -p midi-forge-core` — fail (module missing)**
- [x] **Step 3: Implement `UmpMessage`**
- [x] **Step 4: Tests pass**
- [x] **Step 5: Commit with workspace**

---

### Task 3: MIDI 1.0 stream parser

**Files:**
- Create: `crates/midi-forge-core/src/midi1.rs`

- [x] **Step 1: Failing tests** — note-on, running status, clock between data bytes, 4-byte SysEx7 complete, 8-byte SysEx7 start+end
- [x] **Step 2: Implement `Midi1Parser`**
- [x] **Step 3: Tests pass**

---

### Task 4: Event + decode

**Files:**
- Create: `crates/midi-forge-core/src/event.rs`
- Create: `crates/midi-forge-core/src/decode.rs`

- [x] **Step 1: Tests** — decode note-on and clock from packed UMP
- [x] **Step 2: Implement `MidiEvent` + `decode`**
- [x] **Step 3: Tests pass**

---

### Task 5: IO trait + NullBackend + WinMM list

**Files:**
- Create: `crates/midi-forge-io/src/backend.rs`
- Create: `crates/midi-forge-io/src/null.rs`
- Create: `crates/midi-forge-io/src/winmm.rs` (Windows)

- [x] **Step 1: Test NullBackend lists two fake ports**
- [x] **Step 2: Implement trait + NullBackend**
- [x] **Step 3: WinMM `midiInGetNumDevs` / `midiOutGetNumDevs` enumeration**
- [x] **Step 4: `cargo test -p midi-forge-io` passes**

---

### Task 6: Desktop app

**Files:**
- Create: `crates/midi-forge-app/src/app.rs`
- Modify: `crates/midi-forge-app/src/main.rs`

- [x] **Step 1: `--list` prints endpoints to stdout**
- [x] **Step 2: eframe window shows the same list**
- [x] **Step 3: `cargo build -p midi-forge-app` succeeds**

---

### Task 7: Verify

- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace --all-targets -- -D warnings` (keep clean enough to ship)
- [x] `cargo run -p midi-forge-app -- --list` shows the USB keyboard if Windows sees it

---

## Done when

- Workspace builds on Windows GNU/MSVC.
- Core tests cover UMP packing, running status, interleaved clock, SysEx7 chunking.
- `midi-forge --list` names the connected USB MIDI keyboard (or explains why WinMM sees nothing).
- App window opens with those names. No live streaming yet.
