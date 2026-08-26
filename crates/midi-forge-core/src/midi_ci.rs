use crate::sysex::SysexDump;

/// MIDI-CI Discovery Inquiry (universal non-realtime, broadcast dest MUID).
pub fn discovery_inquiry(source_muid: [u8; 4]) -> SysexDump {
    let mut b = vec![0xF0, 0x7E, 0x7F, 0x0D, 0x70, 0x01];
    b.extend_from_slice(&source_muid);
    b.extend_from_slice(&[0x7F, 0x7F, 0x7F, 0x7F]); // dest = broadcast
    b.extend_from_slice(&[0x00, 0x00, 0x00]); // manufacturer unknown
    b.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // family / model
    b.extend_from_slice(&[0x00, 0x00, 0x01, 0x00]); // software
    b.push(0x0C); // category: likely MIDI 1 + 2 / protocol negotiation
    b.extend_from_slice(&[0x00, 0x20, 0x00, 0x00]); // max sysex ~4096
    b.push(0xF7);
    SysexDump::from_bytes(b).expect("framed CI inquiry")
}

/// Midi-Forge default 28-bit MUID packed as 4×7-bit bytes.
pub const FORGE_MUID: [u8; 4] = [0x0D, 0x46, 0x47, 0x01]; // "MFG" flavour

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CiDiscovery {
    pub is_reply: bool,
    pub source_muid: [u8; 4],
    pub manufacturer: Vec<u8>,
    pub family: u16,
    pub model: u16,
}

impl CiDiscovery {
    pub fn summary(&self) -> String {
        let kind = if self.is_reply {
            "CI Discovery reply"
        } else {
            "CI Discovery inquiry"
        };
        let mfr = crate::mfr::manufacturer_label(&self.manufacturer);
        format!(
            "{kind} {mfr} family {family:04X} model {model:04X} muid {:02X}{:02X}{:02X}{:02X}",
            self.source_muid[0],
            self.source_muid[1],
            self.source_muid[2],
            self.source_muid[3],
            family = self.family,
            model = self.model
        )
    }
}

pub fn parse_ci_discovery(dump: &SysexDump) -> Option<CiDiscovery> {
    let p = dump.payload();
    // 7E nn 0D 70/71 ver srcMUID(4) destMUID(4) mfr…
    if p.len() < 20 || p[0] != 0x7E || p[2] != 0x0D {
        return None;
    }
    let sub = p[3];
    if sub != 0x70 && sub != 0x71 {
        return None;
    }
    let source_muid = [p[5], p[6], p[7], p[8]];
    let rest = &p[13..];
    if rest.len() < 7 {
        return None;
    }
    Some(CiDiscovery {
        is_reply: sub == 0x71,
        source_muid,
        manufacturer: rest[..3].to_vec(),
        family: u16::from(rest[3]) | (u16::from(rest[4]) << 8),
        model: u16::from(rest[5]) | (u16::from(rest[6]) << 8),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inquiry_roundtrip_parse() {
        let dump = discovery_inquiry(FORGE_MUID);
        let ci = parse_ci_discovery(&dump).unwrap();
        assert!(!ci.is_reply);
        assert_eq!(ci.source_muid, FORGE_MUID);
    }

    #[test]
    fn reply_yamaha_shape() {
        let mut b = vec![0xF0, 0x7E, 0x7F, 0x0D, 0x71, 0x01];
        b.extend_from_slice(&[1, 2, 3, 4]);
        b.extend_from_slice(&[0x7F, 0x7F, 0x7F, 0x7F]);
        b.extend_from_slice(&[0x43, 0x00, 0x00]);
        b.extend_from_slice(&[0x01, 0x00, 0x02, 0x00]);
        b.extend_from_slice(&[0; 8]);
        b.push(0xF7);
        let dump = SysexDump::from_bytes(b).unwrap();
        let ci = parse_ci_discovery(&dump).unwrap();
        assert!(ci.is_reply);
        assert!(ci.summary().contains("Yamaha"));
    }
}
