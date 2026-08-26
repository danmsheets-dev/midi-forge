# P0 Bench tools

**Goal:** Daily-driver MIDI-OX bench features MIDI-OX lacked: inject, monitor tools, stuck notes, mute clock, activity, hotplug toast.

## File map

| Path | Role |
|------|------|
| `crates/midi-forge-core/src/hang.rs` | Sounding-note tracker |
| `crates/midi-forge-app/src/inject.rs` | Keyboard + CC inject |
| Monitor / banner / endpoints | Filter, export, mute clock, dots, hotplug |

### Tasks

- [x] HangTracker (note on/off, all-notes-off, cap)
- [x] Keyboard + CC send to an open output
- [x] Monitor type/channel/search + copy/export
- [x] Stuck-note list; panic also note-offs hanging notes
- [x] Banner mute clock (thru only)
- [x] Activity dots; 2s device fingerprint hotplug rescan
