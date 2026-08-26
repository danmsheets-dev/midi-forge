# P1 Firmware / vintage bench

**Goal:** Safer SysEx dumps and less firehose to old hardware.

| Item | Where |
|------|--------|
| Manufacturer names on identity | `sysex.rs` |
| Hex diff of two dumps | core + SysEx panel |
| Handshake (wait for F7) + identity wizard + retry | librarian send job |
| Thru short-message gap | app drain + tick |

- [x] `manufacturer_name`
- [x] `hex_diff`
- [x] Handshake send / identity wizard
- [x] Thru throttle
