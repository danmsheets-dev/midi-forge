# Remaining technician items

**Goal:** MIDI-CI PE/profile inquiry, dump packs, named scenes, MPE mode, DAW-visible loopbacks via `midi.exe`. Still no fake WinRT `MidiSession`.

| Item | Where |
|------|--------|
| CI Profile + PE capabilities | `midi_ci.rs` |
| Dump packs (GM/GS/XG, Sequential, Korg, Yamaha) | `packs.rs` |
| Scenes in profile JSON | `profile.rs` + banner |
| MPE on/off + PB range send | `mpe.rs` |
| `midi loopback create --root-name` | `wms.rs` + virtual cables |

- [x] CI PE / profiles
- [x] Dump packs
- [x] Scenes
- [x] MPE status / PB range
- [x] WMS loopback CLI
