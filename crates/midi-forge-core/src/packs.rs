use crate::sysex::SysexDump;

/// Named SysEx a tech can fire without typing hex.
#[derive(Clone, Copy, Debug)]
pub struct DumpPack {
    pub name: &'static str,
    pub blurb: &'static str,
    pub bytes: &'static [u8],
}

pub fn dump_packs() -> &'static [DumpPack] {
    &[
        DumpPack {
            name: "GM System On",
            blurb: "Universal GM reset",
            bytes: &[0xF0, 0x7E, 0x7F, 0x09, 0x01, 0xF7],
        },
        DumpPack {
            name: "GM2 System On",
            blurb: "Universal GM2 reset",
            bytes: &[0xF0, 0x7E, 0x7F, 0x09, 0x03, 0xF7],
        },
        DumpPack {
            name: "GS Reset",
            blurb: "Roland GS (checksum 41)",
            bytes: &[
                0xF0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7F, 0x00, 0x41, 0xF7,
            ],
        },
        DumpPack {
            name: "XG System On",
            blurb: "Yamaha XG",
            bytes: &[0xF0, 0x43, 0x10, 0x4C, 0x00, 0x00, 0x7E, 0x00, 0xF7],
        },
        DumpPack {
            name: "Yamaha dump request",
            blurb: "Generic Yamaha bulk request (device 0). Many models need a specific ID.",
            bytes: &[
                0xF0, 0x43, 0x20, 0x7A, 0x4C, 0x4D, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0xF7,
            ],
        },
        DumpPack {
            name: "Korg dump request",
            blurb: "Korg current-dump (ch 1). Family byte 00 is generic.",
            bytes: &[0xF0, 0x42, 0x30, 0x00, 0x10, 0xF7],
        },
        DumpPack {
            name: "Sequential Rev2 dump",
            blurb: "Prophet Rev2 program dump request",
            bytes: &[0xF0, 0x01, 0x2F, 0x05, 0xF7],
        },
        DumpPack {
            name: "Sequential Prophet-6 dump",
            blurb: "Prophet-6 program dump request",
            bytes: &[0xF0, 0x01, 0x2E, 0x05, 0xF7],
        },
        DumpPack {
            name: "Sequential OB-6 dump",
            blurb: "OB-6 program dump request",
            bytes: &[0xF0, 0x01, 0x2D, 0x05, 0xF7],
        },
    ]
}

pub fn pack_dump(pack: &DumpPack) -> SysexDump {
    SysexDump::from_bytes(pack.bytes.to_vec()).expect("framed dump pack")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysex::roland_checksum_from_sum;

    #[test]
    fn packs_frame_and_gs_checksum() {
        for p in dump_packs() {
            let d = pack_dump(p);
            assert_eq!(*d.bytes().first().unwrap(), 0xF0);
            assert_eq!(*d.bytes().last().unwrap(), 0xF7);
        }
        let gs = dump_packs().iter().find(|p| p.name == "GS Reset").unwrap();
        let payload = &gs.bytes[1..gs.bytes.len() - 1];
        // Roland: sum address+data (skip 41 10 42 12), last byte is checksum
        let body = &payload[4..payload.len() - 1];
        let sum: u32 = body.iter().map(|&b| u32::from(b)).sum();
        assert_eq!(payload[payload.len() - 1], roland_checksum_from_sum(sum));
    }
}
