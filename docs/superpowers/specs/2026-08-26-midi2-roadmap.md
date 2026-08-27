# Midi-Forge MIDI 2.0 + 15-feature roadmap

Date: 2026-08-26  
Status: accepted  
Product: Midi-Forge 0.1 → 0.2

This round of upgrades ships **phase 1 of the first five features** and **plans full MIDI 2.0** so later phases plug into types that already exist. Native WinRT `MidiSession` I/O is phase 2 of feature 1 — the App SDK is not a crates.io crate.

## First five (this upgrade series)

| # | Feature | Phase 1 (this PR) | Later |
|---|---------|-------------------|--------|
| 1 | Native WMS `MidiSession` | `BackendCaps`, UMP-preserving loopback, WinMM stays 7-bit fallback | WinRT session, scheduled send, DAW-visible UMP devices |
| 2 | Engine thread | Dedicated `midi-engine` thread owns backend poll/send + clock master; UI snapshots | Lua timers on the same thread |
| 3 | MIDI 2 language | Decode + construct per-note, registered/assignable controllers; 32-bit maps; inject M2 | Flex Data, MixData, JR timestamps, UMP Stream / function blocks |
| 4 | Clock master | Generate clock / start / stop / continue / SPP to a chosen output | Clock fallback if input dies, tap tempo, MTC generate |
| 5 | Headless CLI | `send` / `receive` / `identity` / `panic` / `clock` / `--list` | `route --profile`, dump librarian |

## Remaining ten (not this phase)

6. MIDI-CI Property Exchange GET/SET  
7. SysEx device library  
8. Lua timers + persistent state  
9. Session recorder / SMF  
10. Network MIDI 2.0  
11. ShowMIDI-quality Live + CLAP/VST3  
12. OSC / Web surface  
13. Translator preset pack  
14. MIDI spy / multi-client tap  
15. Saved workspaces / detachable monitor  

## Full MIDI 2.0 plan (types we will grow into)

Canonical event remains `UmpMessage`. Completeness checklist:

| UMP type | Name | Phase 1 | Full MIDI 2 |
|----------|------|---------|-------------|
| 0x0 | Utility (JR clock, NOOP) | decode as Other | schedule + jitter |
| 0x1 | System | done | generate MTC |
| 0x2 | MIDI 1.0 channel voice | done | keep as projection |
| 0x3 | SysEx7 | done | — |
| 0x4 | MIDI 2.0 channel voice | notes/CC/PC/pressure/bend **plus** per-note + RC/AC | attribute types, relative RC/AC |
| 0x5 | SysEx8 / MixData | Other | librarian + 8-bit dumps |
| 0xD | Flex Data | Other | tempo, lyrics, time sig |
| 0xF | UMP Stream | Other | endpoint discovery, function blocks, protocol negotiation |

I/O: WinMM always downscales type 0x4 → 0x2. In-app loopback and a future WMS backend pass words unchanged (`BackendCaps.native_ump`).

Property Exchange, Network MIDI 2.0, and Flex Data stay out of phase 1 so the engine thread and clock master remain testable without WinRT.
