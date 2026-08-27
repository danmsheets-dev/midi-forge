# Midi-Forge MCP Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> After approval, copy to `docs/superpowers/plans/2026-08-27-midi-forge-mcp.md`.

**Goal:** Let a local AI agent (Cursor, Claude Code, Grok) inspect and, when armed, poke the same MIDI session a technician already has open — without sitting on the MIDI callback and without SysEx librarian access.

**Architecture:** Tools talk to a `ForgeMcp` façade over `Arc<Mutex<EngineInner>>` (live GUI) or a standalone `MidiBackend` (headless). A dedicated tokio thread runs official `rmcp` 3.x. Stdio (`midi-forge mcp`) is what IDEs spawn. The GUI optionally binds **127.0.0.1 only** streamable HTTP so the stdio process can **attach** instead of opening ports a second time (WinMM exclusive-open). Writes require `agent_armed`. MIDI engine tick never awaits MCP.

**Tech Stack:** Existing Midi-Forge crates. `rmcp` 3.1 (official Rust MCP SDK, spec 2026-07-28) with `server`, `macros`, `transport-io`, `transport-streamable-http-server`. `tokio` runtime on a side thread. No Node, no MCPB (the app binary *is* the runtime).

---

## Why this shape

| Option | Verdict |
|--------|---------|
| Cloud / remote MCP | Rejected — MIDI ports are this PC |
| GUI process stdio | Rejected — `#![windows_subsystem = "windows"]` has no useful stdin |
| Second MIDI stack in the MCP process | Rejected if GUI already holds WinMM ports |
| Tools on the 1 ms engine tick | Rejected — lock, snapshot, return |
| SysEx dump / PE SET in v1 | Rejected — too easy to brick vintage gear |

v1 is a **technician copilot**, not a DAW.

---

## File map

| Path | Role |
|------|------|
| `crates/midi-forge-app/src/mcp/mod.rs` | `ForgeMcp` server, tool list, arm flag |
| `crates/midi-forge-app/src/mcp/tools.rs` | Tool handlers (pure-ish; take `&mut dyn McpHost`) |
| `crates/midi-forge-app/src/mcp/host.rs` | `McpHost` trait + `LiveHost` (`EngineInner`) + `StandaloneHost` |
| `crates/midi-forge-app/src/mcp/stdio.rs` | `midi-forge mcp` / `midi-forge mcp --attach` |
| `crates/midi-forge-app/src/mcp/http.rs` | 127.0.0.1 streamable HTTP when GUI Agent is on |
| `crates/midi-forge-app/src/main.rs` | Dispatch `mcp` like CLI; attach console |
| `crates/midi-forge-app/src/app.rs` | `agent_listen`, `agent_armed`, banner checkbox |
| `crates/midi-forge-app/Cargo.toml` | `rmcp`, `tokio`, `axum` if HTTP feature needs it |
| `README.md` | Cursor / Claude MCP JSON snippet |

Do **not** add a new workspace crate unless `midi-forge-app` tests cannot run without eframe. Prefer unit tests of `tools.rs` against `StandaloneHost` + `NullBackend`.

---

## Key decisions

1. **Same binary.** `midi-forge mcp` is a CLI mode next to `send` / `receive`. GUI stays the default with no args.
2. **Two hosts, one tool table.** `McpHost` methods: list endpoints, tail log, live dump, clock, stuck, thru, snapshot, send UMP, identity, panic, open/close port. Live host locks `EngineInner` for microseconds. Standalone host owns a `MidiBackend` like `cli.rs`.
3. **Attach first for technicians.** `midi-forge mcp --attach` (default if `http://127.0.0.1:7420/mcp` answers). Else standalone. Document that two processes cannot both open the same WinMM input.
4. **Loopback bind only.** HTTP `127.0.0.1:7420`. Never `0.0.0.0`. Port override `--mcp-port`.
5. **Arm writes.** GUI: **Agent** checkbox (listen) + **Arm writes** checkbox (default off). Standalone: writes fail unless `--arm`. Tools return a clear error if unarmed.
6. **No SysEx librarian / PE SET / profile load** in v1.
7. **Tokio off the MIDI path.** `std::thread::Builder` named `midi-mcp` runs `Runtime::new().block_on(...)`. Engine thread unchanged.
8. **Decoded English + UMP words** in every event tool (reuse `decode()` + hex of `words()`).
9. **`rmcp` 3.x**, not a hand-rolled JSON-RPC. If `rmcp` HTTP is too heavy for the first PR, ship **stdio + attach via stdio-to-HTTP proxy in-process** still using rmcp HTTP client; do not invent a private protocol.

---

## Tool list (v1, freeze this)

Read (always):

| Tool | Returns |
|------|---------|
| `list_endpoints` | id, name, direction, protocol, open |
| `monitor_tail` | `limit` (default 40, max 200): time, port, ump words, decoded summary |
| `live_now` | per-channel sounding / last CC / bend (existing `LiveView::dump`) |
| `clock_health` | `ClockHealth::summary` + master BPM if enabled |
| `stuck_notes` | hang list |
| `thru_graph` | each link from→to + filter flags that are off |
| `mpe_status` | `mode_summary` + sounding voices |
| `snapshot` | `EngineInner::snapshot_text()` |

Write (need arm):

| Tool | Args |
|------|------|
| `send_note` | `out` (name or id), `note`, `vel`, `ch`, `group`, `m2` |
| `send_cc` | `out`, `cc`, `val`, `ch`, `group`, `m2` |
| `identity` | `out` |
| `panic` | `out` optional (default all open outputs, else all outputs) |
| `set_port_open` | `id`, `input`/`output`, `open` |

Out of v1: SysEx dump, PE GET/SET, Lua apply, scene load, clock master start (easy to surprise a drum machine).

---

## Task 1 — `McpHost` + tool handlers (TDD, no network)

**Files:** create `mcp/host.rs`, `mcp/tools.rs`, `mcp/mod.rs`

- [ ] **Step 1: Failing tests** in `tools.rs`:

```rust
#[test]
fn monitor_tail_returns_decoded_note() {
    let mut host = StandaloneHost::with_null();
    host.push_note(); // inject a type-2 note-on into the log
    let json = crate::mcp::tools::monitor_tail(&mut host, 10).unwrap();
    assert!(json.contains("NoteOn"));
    assert!(json.contains("2090"));
}

#[test]
fn send_note_refuses_when_unarmed() {
    let mut host = StandaloneHost::with_null();
    let err = crate::mcp::tools::send_note(&mut host, SendNote { .. }).unwrap_err();
    assert!(err.contains("arm"));
}
```

- [ ] **Step 2:** Run `cargo test -p midi-forge-app monitor_tail_returns_decoded_note -- --nocapture` — fail (module missing).

- [ ] **Step 3:** Implement `McpHost` trait and handlers. `StandaloneHost` uses `NullBackend` in tests (`midi_forge_io::NullBackend`). Live host is a later task wrapping `EngineInner`.

- [ ] **Step 4:** Tests pass. `cargo test -p midi-forge-app`.

- [ ] **Step 5:** Commit `feat(mcp): host trait and technician tool handlers`

---

## Task 2 — Stdio server `midi-forge mcp`

**Files:** `mcp/stdio.rs`, `main.rs`, `cli.rs` dispatch

- [ ] Wire `mcp` into `cli::dispatch` **or** a sibling check in `main` (keep GUI default).
- [ ] `attach_parent_console()` like `--list`.
- [ ] `rmcp` stdio transport on a tokio runtime (can be current-thread in this process — no GUI).
- [ ] Flags: `--attach`, `--arm`, `--mcp-url http://127.0.0.1:7420/mcp`.
- [ ] Default: try attach (TCP connect 100 ms); on failure, standalone `default_backend()` and auto-open inputs like the GUI.
- [ ] Test: spawn is hard in unit tests; test flag parsing + “unarmed send” via host. Optional `#[ignore]` stdio smoke.

- [ ] Commit `feat(mcp): stdio midi-forge mcp mode`

Cursor snippet (README later):

```json
{
  "mcpServers": {
    "midi-forge": {
      "command": "C:\\\\path\\\\to\\\\midi-forge.exe",
      "args": ["mcp", "--attach"]
    }
  }
}
```

`--attach` with GUI down should fall back to standalone and print that on stderr once.

---

## Task 3 — GUI listen + arm + HTTP

**Files:** `mcp/http.rs`, `app.rs` banner, `MidiForgeApp::new`

- [ ] Fields: `agent_listen: bool`, `agent_armed: bool`, `agent_port: u16` (7420), `agent_status: String`.
- [ ] Banner: checkboxes **Agent** / **Arm writes**. Hover: “127.0.0.1:7420 MCP. Writes disabled until Arm.”
- [ ] When Agent turns on, spawn `midi-mcp` thread: tokio + `rmcp` streamable HTTP **bind 127.0.0.1**. `LiveHost` = `Arc<Mutex<EngineInner>>` already on `MidiForgeApp`.
- [ ] When Agent turns off, signal shutdown (watch/Notify). Do not leak threads.
- [ ] Tool handlers `lock()` the mutex; never hold it across `.await`. Copy strings out, then return.
- [ ] Writes check `agent_armed` on the inner state.

- [ ] Test: `packets_for_wire`-style unit test that LiveHost send_note is a no-op when unarmed (can use a tiny fake host). HTTP bind test: listen on 127.0.0.1:0, drop.

- [ ] Commit `feat(mcp): GUI localhost MCP when Agent is enabled`

---

## Task 4 — Attach path must share the live session

**Files:** `mcp/stdio.rs`

- [ ] `--attach`: stdio MCP server whose tools are **HTTP client** calls to the GUI (rmcp streamable HTTP client **or** JSON-RPC POST if that is what rmcp 3 serves). Same tool names.
- [ ] If GUI not listening: standalone fallback (Task 2).
- [ ] Document WinMM: do not open the same input in two processes.

This is the technician path: GUI open on the bench, Cursor attached.

- [ ] Commit `feat(mcp): stdio --attach proxies to GUI session`

---

## Task 5 — Docs + dist

- [ ] README: Agent checkboxes, `midi-forge mcp --attach`, Cursor JSON, “no SysEx”, loopback-only HTTP.
- [ ] `midi-forge help` lists `mcp`.
- [ ] Rebuild `dist/midi-forge.exe`.

- [ ] Commit `docs: MCP agent port for technicians`

---

## Testing strategy

| Layer | How |
|-------|-----|
| Tools | `NullBackend` fixtures, no hardware |
| Arm | unarmed write → error string contains `arm` |
| Bind | 127.0.0.1 only; connecting to 0.0.0.0 must not work |
| Workspace | `cargo test --workspace` without MIDI devices |
| Manual | GUI Agent on, `mcp --attach`, `list_endpoints` from an MCP inspector |

---

## Out of scope

- Remote MCP, auth, MCPB, Node
- SysEx librarian, PE SET, Lua apply, scene recall
- Agent-driven clock master start/stop
- Sampling / elicitation
- Opening ports the GUI does not already have, without `set_port_open` (allowed but armed)

---

## PR / commit sequence

1. Host + tools + tests  
2. Stdio standalone  
3. GUI HTTP + arm  
4. `--attach`  
5. README + help  

3 and 4 can swap if attach is implemented as “stdio always standalone” first; **do not ship README that tells technicians to run two MIDI stacks**.

---

## Self-review vs product brief

| Brief | Task |
|-------|------|
| Local only | HTTP 127.0.0.1, no cloud |
| Same binary | `midi-forge mcp` |
| Live session | `--attach` + GUI Agent |
| Engine never blocks on LLM | tokio side thread, short mutex |
| Armed writes | Task 1 + 3 |
| No SysEx v1 | tool list freeze |
| Decoded + UMP | `monitor_tail` |
