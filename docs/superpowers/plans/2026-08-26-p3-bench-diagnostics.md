# P3 Bench diagnostics (MIDI-OX never had this)

**Goal:** Clock/MTC health, occupancy copy, receive-interval histogram, thru merge/split log, snapshot, MIDI learn, always-on-top + big Panic.

Host receive timing only — not cable delay. WinMM cannot name the other process; we guess from visible window titles.

| Item | Where |
|------|--------|
| Clock / MTC / SPP / runaway | `timing.rs` + Clock panel |
| Note + clock interval histogram | same |
| Exclusive-open occupancy | `occupy.rs` + open errors |
| Thru path log | `route.rs` |
| Snapshot | Live / banner |
| MIDI learn | `Matcher::learn_from` + thru map |
| Always-on-top + PANIC | banner |

- [x] ClockHealth + histogram
- [x] Occupancy explain
- [x] Route log
- [x] Snapshot
- [x] MIDI learn
- [x] Always-on-top + big Panic
