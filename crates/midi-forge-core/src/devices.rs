//! Named hardware dump profiles (delay, handshake, identity prefix).

use crate::packs::{DumpPack, dump_packs};
use crate::sysex::SysexDump;

#[derive(Clone, Copy, Debug)]
pub struct DeviceProfile {
    pub name: &'static str,
    pub maker: &'static str,
    pub delay_ms: u32,
    pub handshake: bool,
    pub identity_prefix: &'static [u8],
    pub pack_name: &'static str,
}

pub fn device_library() -> &'static [DeviceProfile] {
    &[
        DeviceProfile {
            name: "Roland GS / JV",
            maker: "Roland",
            delay_ms: 20,
            handshake: true,
            identity_prefix: &[0xF0, 0x7E, 0x10, 0x06, 0x02, 0x41],
            pack_name: "GS Reset",
        },
        DeviceProfile {
            name: "Yamaha XG",
            maker: "Yamaha",
            delay_ms: 15,
            handshake: true,
            identity_prefix: &[0xF0, 0x7E],
            pack_name: "XG System On",
        },
        DeviceProfile {
            name: "Sequential Rev2",
            maker: "Sequential",
            delay_ms: 40,
            handshake: true,
            identity_prefix: &[0xF0, 0x01, 0x2F],
            pack_name: "Sequential Rev2 dump",
        },
        DeviceProfile {
            name: "Prophet-6",
            maker: "Sequential",
            delay_ms: 40,
            handshake: true,
            identity_prefix: &[0xF0, 0x01, 0x2E],
            pack_name: "Sequential Prophet-6 dump",
        },
        DeviceProfile {
            name: "Korg (generic)",
            maker: "Korg",
            delay_ms: 25,
            handshake: true,
            identity_prefix: &[0xF0, 0x42],
            pack_name: "Korg dump request",
        },
        DeviceProfile {
            name: "GM module",
            maker: "Universal",
            delay_ms: 10,
            handshake: false,
            identity_prefix: &[0xF0, 0x7E],
            pack_name: "GM System On",
        },
    ]
}

pub fn pack_for_device(dev: &DeviceProfile) -> Option<&'static DumpPack> {
    dump_packs().iter().find(|p| p.name == dev.pack_name)
}

pub fn apply_device(dev: &DeviceProfile) -> (u32, bool, SysexDump) {
    let pack = pack_for_device(dev).expect("pack name");
    (dev.delay_ms, dev.handshake, crate::packs::pack_dump(pack))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_device_has_a_pack() {
        for d in device_library() {
            assert!(pack_for_device(d).is_some(), "{}", d.name);
        }
    }
}
