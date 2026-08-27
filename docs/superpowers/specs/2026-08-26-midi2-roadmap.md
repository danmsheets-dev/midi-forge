# Midi-Forge MIDI 2.0 + 15-feature roadmap

Date: 2026-08-26 (updated 2026-08-27)  
Status: accepted — Full MIDI 2 **language** is done; live `MidiSession` I/O is bound (needs App SDK at runtime)  
Product: Midi-Forge 0.1 → 0.2

Phase 1 of the first five features shipped first. Full MIDI 2.0 decode/construct for every defined UMP type is now in tree (see table). Native WinRT `MidiSession` send/receive is bound from the vendored `Windows.Devices.Midi2` winmd: `WmsInit` COM-bootstraps the App SDK (`winget install Microsoft.WindowsMIDIServicesSDK`), then `WmsBackend::try_new` activates `MidiSession`. If WinRT activation fails, WinMM remains the fallback — the backend never claims `native_ump` without a live session.

## First five (this upgrade series)

| # | Feature | Phase 1 (this PR) | Later |
|---|---------|-------------------|--------|
| 1 | Native WMS `MidiSession` | `BackendCaps`, UMP-preserving loopback, WinMM stays 7-bit fallback | **done:** live `MidiSession` enumerate/open/send/receive (COM raw words). Scheduled send still later. Runtime requires App SDK. |
| 2 | Engine thread | Dedicated `midi-engine` thread owns backend poll/send + clock master; UI snapshots | Lua timers on the same thread |
| 3 | MIDI 2 language | Decode + construct per-note, registered/assignable controllers; 32-bit maps; inject M2 | **done:** Flex Data, MixData, JR timestamps, UMP Stream / function blocks |
| 4 | Clock master | Generate clock / start / stop / continue / SPP to a chosen output | Clock fallback if input dies, tap tempo, MTC generate |
| 5 | Headless CLI | `send` / `receive` / `identity` / `panic` / `clock` / `--list` | `route --profile`, dump librarian |

## Remaining ten (not this phase)

Items 6–9 and 11–15 have partial UI (PE inquiry, device library, Lua timers, SMF0, Live view, scenes). Still remaining as originally scoped:

6. MIDI-CI Property Exchange GET/SET (full JSON session)  
7. SysEx device library (deeper than the current pack/identity)  
8. Lua timers + persistent state (partial)  
9. Session recorder / SMF (SMF0 done; MIDI Clip File / SMF2 remaining)  
10. Network MIDI 2.0 (UDP invitation + datagrams exist; session/auth remaining)  
11. ShowMIDI-quality Live + CLAP/VST3  
12. OSC / Web surface  
13. Translator preset pack  
14. MIDI spy / multi-client tap  
15. Saved workspaces / detachable monitor  

## Full MIDI 2.0 (types)

Canonical event remains `UmpMessage`. Completeness checklist:

| UMP type | Name | Phase 1 | Full MIDI 2 |
|----------|------|---------|-------------|
| 0x0 | Utility (JR clock, NOOP) | decode as Other | **done** (decode + construct). Schedule + jitter still needs `MidiSession` `scheduled_send`. |
| 0x1 | System | done | generate MTC (receive exists; generate not blocking MIDI 2) |
| 0x2 | MIDI 1.0 channel voice | done | **done** as projection + upscale |
| 0x3 | SysEx7 | done | **done** |
| 0x4 | MIDI 2.0 channel voice | notes/CC/PC/pressure/bend **plus** per-note + RC/AC | **done** (attribute types, relative RC/AC) |
| 0x5 | SysEx8 / MixData | Other | **done** (assembler + MixData decode; 8-bit dumps on UMP dests only) |
| 0xD | Flex Data | Other | **done** (tempo, lyrics, time sig) |
| 0xF | UMP Stream | Other | **done** (endpoint discovery, function blocks, protocol negotiation) |

I/O: MIDI 1 endpoints (WinMM) downscale type 0x4 → 0x2. UMP dests (WMS `MidiSession`, in-app loopback, CoreMIDI MIDI 2) pass type 0x4 unchanged (`packets_for_wire` / `BackendCaps.native_ump`). `WmsBackend::try_new` fails closed to WinMM when the App SDK is not registered.

Property Exchange (full JSON GET/SET), Network MIDI 2.0 session state, and MIDI Clip File / SMF2 remain out of this MIDI 2 language pass.
