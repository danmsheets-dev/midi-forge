use crate::sysex::SysexDump;

const BROADCAST: [u8; 4] = [0x7F, 0x7F, 0x7F, 0x7F];

fn ci_frame(sub: u8, source_muid: [u8; 4], rest: &[u8]) -> SysexDump {
    let mut b = vec![0xF0, 0x7E, 0x7F, 0x0D, sub, 0x01];
    b.extend_from_slice(&source_muid);
    b.extend_from_slice(&BROADCAST);
    b.extend_from_slice(rest);
    b.push(0xF7);
    SysexDump::from_bytes(b).expect("framed CI")
}

/// MIDI-CI Discovery Inquiry (universal non-realtime, broadcast dest MUID).
pub fn discovery_inquiry(source_muid: [u8; 4]) -> SysexDump {
    let mut rest = Vec::from([0x00, 0x00, 0x00]); // manufacturer unknown
    rest.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // family / model
    rest.extend_from_slice(&[0x00, 0x00, 0x01, 0x00]); // software
    rest.push(0x0C); // category: MIDI 1 + 2 / protocol negotiation
    rest.extend_from_slice(&[0x00, 0x20, 0x00, 0x00]); // max sysex ~4096
    ci_frame(0x70, source_muid, &rest)
}

/// Profile Configuration Inquiry (entire port / all channels).
pub fn profile_inquiry(source_muid: [u8; 4]) -> SysexDump {
    ci_frame(0x20, source_muid, &[0x7F])
}

/// Property Exchange Capability Inquiry (v1.2: 1-byte simultaneous requests).
pub fn pe_capability_inquiry(source_muid: [u8; 4]) -> SysexDump {
    ci_frame(0x30, source_muid, &[1])
}

fn u14(n: usize) -> [u8; 2] {
    [(n & 0x7F) as u8, ((n >> 7) & 0x7F) as u8]
}

fn read_u14(b: &[u8]) -> Option<usize> {
    Some(usize::from(*b.first()?) | (usize::from(*b.get(1)?) << 7))
}

/// MIDI-CI PE Get Property Data Inquiry (0x34).
pub fn pe_get(source_muid: [u8; 4], request_id: u8, header: &str) -> SysexDump {
    let hdr = header.as_bytes();
    let mut rest = vec![request_id & 0x7F];
    rest.extend_from_slice(&u14(hdr.len()));
    rest.extend_from_slice(hdr);
    ci_frame(0x34, source_muid, &rest)
}

/// MIDI-CI PE Set Property Data Inquiry (0x36).
pub fn pe_set(source_muid: [u8; 4], request_id: u8, header: &str, body: &[u8]) -> SysexDump {
    let hdr = header.as_bytes();
    let mut rest = vec![request_id & 0x7F];
    rest.extend_from_slice(&u14(hdr.len()));
    rest.extend_from_slice(hdr);
    rest.extend_from_slice(&u14(body.len()));
    rest.extend_from_slice(body);
    ci_frame(0x36, source_muid, &rest)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeData {
    pub is_set: bool,
    pub is_reply: bool,
    pub source_muid: [u8; 4],
    pub request_id: u8,
    pub header: String,
    pub body: Vec<u8>,
}

impl PeData {
    pub fn summary(&self) -> String {
        let kind = match (self.is_set, self.is_reply) {
            (false, false) => "PE GET",
            (false, true) => "PE GET reply",
            (true, false) => "PE SET",
            (true, true) => "PE SET reply",
        };
        format!(
            "{kind} id {} {} ({} B)",
            self.request_id,
            self.header,
            self.body.len()
        )
    }
}

pub fn parse_pe_data(dump: &SysexDump) -> Option<PeData> {
    let (sub, source_muid, rest) = ci_parts(dump)?;
    let (is_set, is_reply) = match sub {
        0x34 => (false, false),
        0x35 => (false, true),
        0x36 => (true, false),
        0x37 => (true, true),
        _ => return None,
    };
    if rest.is_empty() {
        return None;
    }
    let request_id = rest[0];
    let hlen = read_u14(rest.get(1..3)?)?;
    if rest.len() < 3 + hlen {
        return None;
    }
    let header = String::from_utf8_lossy(&rest[3..3 + hlen]).into_owned();
    let mut body = Vec::new();
    let after = 3 + hlen;
    if rest.len() >= after + 2 {
        let blen = read_u14(&rest[after..after + 2])?;
        let start = after + 2;
        if rest.len() >= start + blen {
            body = rest[start..start + blen].to_vec();
        }
    }
    Some(PeData {
        is_set,
        is_reply,
        source_muid,
        request_id,
        header,
        body,
    })
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

fn ci_parts(dump: &SysexDump) -> Option<(u8, [u8; 4], &[u8])> {
    let p = dump.payload();
    // 7E nn 0D sub ver srcMUID(4) destMUID(4) …
    if p.len() < 13 || p[0] != 0x7E || p[2] != 0x0D {
        return None;
    }
    let source_muid = [p[5], p[6], p[7], p[8]];
    Some((p[3], source_muid, &p[13..]))
}

pub fn parse_ci_discovery(dump: &SysexDump) -> Option<CiDiscovery> {
    let (sub, source_muid, rest) = ci_parts(dump)?;
    if sub != 0x70 && sub != 0x71 {
        return None;
    }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CiProfileList {
    pub source_muid: [u8; 4],
    pub enabled: Vec<[u8; 5]>,
    pub disabled: Vec<[u8; 5]>,
}

impl CiProfileList {
    pub fn summary(&self) -> String {
        format!(
            "CI Profiles enabled {} disabled {}",
            self.enabled.len(),
            self.disabled.len()
        )
    }
}

pub fn parse_ci_profiles(dump: &SysexDump) -> Option<CiProfileList> {
    let (sub, source_muid, rest) = ci_parts(dump)?;
    if sub != 0x21 || rest.len() < 2 {
        return None;
    }
    // M2-101-UM Table 18: after dest MUID, 2-byte LSB-first enabled count, 5-byte IDs,
    // then 2-byte disabled count. No extra dest-channel byte.
    let mut i = 0usize;
    let n_en = usize::from(rest[i]) | (usize::from(*rest.get(i + 1)?) << 7);
    i += 2;
    if i + n_en * 5 > rest.len() {
        return None;
    }
    let mut enabled = Vec::new();
    for _ in 0..n_en {
        let mut id = [0u8; 5];
        id.copy_from_slice(&rest[i..i + 5]);
        enabled.push(id);
        i += 5;
    }
    if i + 2 > rest.len() {
        return Some(CiProfileList {
            source_muid,
            enabled,
            disabled: Vec::new(),
        });
    }
    let n_dis = usize::from(rest[i]) | (usize::from(rest[i + 1]) << 7);
    i += 2;
    if i + n_dis * 5 > rest.len() {
        return None;
    }
    let mut disabled = Vec::new();
    for _ in 0..n_dis {
        let mut id = [0u8; 5];
        id.copy_from_slice(&rest[i..i + 5]);
        disabled.push(id);
        i += 5;
    }
    Some(CiProfileList {
        source_muid,
        enabled,
        disabled,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CiPeCaps {
    pub source_muid: [u8; 4],
    pub simultaneous: u8,
    pub pe_major: u8,
    pub pe_minor: u8,
}

impl CiPeCaps {
    pub fn summary(&self) -> String {
        format!(
            "CI PE v{}.{}  {} simultaneous",
            self.pe_major, self.pe_minor, self.simultaneous
        )
    }
}

pub fn parse_ci_pe_caps(dump: &SysexDump) -> Option<CiPeCaps> {
    let (sub, source_muid, rest) = ci_parts(dump)?;
    if sub != 0x31 || rest.len() < 3 {
        return None;
    }
    Some(CiPeCaps {
        source_muid,
        simultaneous: rest[0],
        pe_major: rest[1],
        pe_minor: rest[2],
    })
}

/// Best-effort CI summary for the SysEx identity line.
pub fn parse_ci_note(dump: &SysexDump) -> Option<String> {
    if let Some(d) = parse_ci_discovery(dump) {
        return Some(d.summary());
    }
    if let Some(p) = parse_ci_profiles(dump) {
        return Some(p.summary());
    }
    if let Some(pe) = parse_ci_pe_caps(dump) {
        return Some(pe.summary());
    }
    let (sub, _, _) = ci_parts(dump)?;
    if sub == 0x20 {
        return Some("CI Profile inquiry".into());
    }
    if sub == 0x30 {
        return Some("CI PE capability inquiry".into());
    }
    if let Some(pe) = parse_pe_data(dump) {
        return Some(pe.summary());
    }
    None
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

    #[test]
    fn pe_and_profile_parse() {
        let pe_inq = pe_capability_inquiry(FORGE_MUID);
        assert!(parse_ci_note(&pe_inq).unwrap().contains("PE"));
        let payload = pe_inq.payload();
        assert_eq!(*payload.last().unwrap(), 1, "simultaneous-requests byte");
        let mut b = vec![0xF0, 0x7E, 0x7F, 0x0D, 0x31, 0x01];
        b.extend_from_slice(&FORGE_MUID);
        b.extend_from_slice(&[0x7F, 0x7F, 0x7F, 0x7F]);
        b.extend_from_slice(&[2, 1, 0]);
        b.push(0xF7);
        let pe = parse_ci_pe_caps(&SysexDump::from_bytes(b).unwrap()).unwrap();
        assert_eq!(pe.simultaneous, 2);
        assert!(pe.summary().contains("PE"));

        let mut p = vec![0xF0, 0x7E, 0x7F, 0x0D, 0x21, 0x01];
        p.extend_from_slice(&FORGE_MUID);
        p.extend_from_slice(&[0x7F, 0x7F, 0x7F, 0x7F]);
        p.extend_from_slice(&[0x01, 0x00]); // 1 enabled, 14-bit LSB first
        p.extend_from_slice(&[0x21, 0x00, 0x01, 0x00, 0x01]);
        p.extend_from_slice(&[0x00, 0x00]); // 0 disabled
        p.push(0xF7);
        let list = parse_ci_profiles(&SysexDump::from_bytes(p).unwrap()).unwrap();
        assert_eq!(list.enabled.len(), 1);
        assert!(list.summary().contains("enabled 1"));
        assert!(
            parse_ci_note(&profile_inquiry(FORGE_MUID))
                .unwrap()
                .contains("Profile")
        );
    }

    #[test]
    fn pe_get_set_roundtrip() {
        let get = pe_get(FORGE_MUID, 3, r#"{"resource":"DeviceInfo"}"#);
        let parsed = parse_pe_data(&get).unwrap();
        assert!(!parsed.is_set && !parsed.is_reply);
        assert_eq!(parsed.request_id, 3);
        assert!(parsed.header.contains("DeviceInfo"));
        let set = pe_set(FORGE_MUID, 4, r#"{"resource":"X"}"#, b"hi");
        let p2 = parse_pe_data(&set).unwrap();
        assert!(p2.is_set);
        assert_eq!(p2.body, b"hi");
    }
}
