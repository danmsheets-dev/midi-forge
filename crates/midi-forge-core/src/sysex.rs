use crate::mfr::manufacturer_name;
use crate::ump::UmpMessage;

/// Universal identity request (non-realtime, all devices).
pub const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SysexError {
    #[error("SysEx dump must start with F0 and end with F7")]
    Framing,
    #[error("no complete SysEx dump (F0…F7) found")]
    Empty,
    #[error("invalid hex: {0}")]
    Hex(String),
}

/// One complete System Exclusive message, including `F0` and `F7`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysexDump {
    bytes: Vec<u8>,
}

impl SysexDump {
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, SysexError> {
        let bytes = bytes.into();
        if bytes.len() < 2 || bytes[0] != 0xF0 || *bytes.last().unwrap() != 0xF7 {
            return Err(SysexError::Framing);
        }
        if bytes[1..bytes.len() - 1].iter().any(|&b| b > 0x7F) {
            return Err(SysexError::Framing);
        }
        Ok(Self { bytes })
    }

    pub fn identity_request() -> Self {
        Self {
            bytes: IDENTITY_REQUEST.to_vec(),
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn payload(&self) -> &[u8] {
        let n = self.bytes.len();
        &self.bytes[1..n - 1]
    }

    pub fn to_hex(&self) -> String {
        self.bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn to_ump_packets(&self, group: u8) -> Vec<UmpMessage> {
        let payload = self.payload();
        if payload.is_empty() {
            return vec![UmpMessage::sysex7(group, 0, &[])];
        }
        let mut out = Vec::new();
        let mut offset = 0;
        let mut first = true;
        while offset < payload.len() {
            let take = (payload.len() - offset).min(6);
            let last = offset + take == payload.len();
            let status = match (first, last) {
                (true, true) => 0,
                (true, false) => 1,
                (false, false) => 2,
                (false, true) => 3,
            };
            out.push(UmpMessage::sysex7(
                group,
                status,
                &payload[offset..offset + take],
            ));
            offset += take;
            first = false;
        }
        out
    }

    /// Set the last payload byte to a Roland-style checksum (128 − sum) & 0x7F
    /// of all payload bytes except that last one.
    pub fn with_roland_checksum(&self) -> Result<Self, SysexError> {
        let mut payload = self.payload().to_vec();
        if payload.is_empty() {
            return Err(SysexError::Framing);
        }
        let last = payload.len() - 1;
        let sum: u32 = payload[..last].iter().map(|&b| u32::from(b)).sum();
        payload[last] = roland_checksum_from_sum(sum);
        let mut bytes = Vec::with_capacity(payload.len() + 2);
        bytes.push(0xF0);
        bytes.extend_from_slice(&payload);
        bytes.push(0xF7);
        Self::from_bytes(bytes)
    }
}

pub fn roland_checksum_from_sum(sum: u32) -> u8 {
    ((0x80u32.wrapping_sub(sum & 0x7F)) & 0x7F) as u8
}

/// Reassemble UMP SysEx7 packets into complete dumps.
#[derive(Default)]
pub struct SysexAssembler {
    buf: Vec<u8>,
    open: bool,
}

impl SysexAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.open = false;
    }

    pub fn push(&mut self, packet: &UmpMessage) -> Option<SysexDump> {
        let (status, count, data) = packet.sysex7_parts()?;
        let n = usize::from(count.min(6));
        match status {
            0 => {
                self.reset();
                SysexDump::from_bytes(frame(&data[..n])).ok()
            }
            1 => {
                self.buf.clear();
                self.buf.extend_from_slice(&data[..n]);
                self.open = true;
                None
            }
            2 if self.open => {
                self.buf.extend_from_slice(&data[..n]);
                None
            }
            3 if self.open => {
                self.buf.extend_from_slice(&data[..n]);
                let dump = SysexDump::from_bytes(frame(&self.buf)).ok();
                self.reset();
                dump
            }
            _ => {
                self.reset();
                None
            }
        }
    }
}

fn frame(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 2);
    out.push(0xF0);
    out.extend_from_slice(payload);
    out.push(0xF7);
    out
}

/// Split a `.syx` blob into complete dumps.
pub fn dumps_from_syx(data: &[u8]) -> Result<Vec<SysexDump>, SysexError> {
    let mut dumps = Vec::new();
    let mut i = 0;
    while i < data.len() {
        while i < data.len() && data[i] != 0xF0 {
            i += 1;
        }
        if i >= data.len() {
            break;
        }
        let start = i;
        i += 1;
        while i < data.len() && data[i] != 0xF7 {
            if data[i] > 0x7F && data[i] != 0xF0 {
                return Err(SysexError::Framing);
            }
            i += 1;
        }
        if i >= data.len() {
            return Err(SysexError::Framing);
        }
        dumps.push(SysexDump::from_bytes(data[start..=i].to_vec())?);
        i += 1;
    }
    if dumps.is_empty() {
        Err(SysexError::Empty)
    } else {
        Ok(dumps)
    }
}

pub fn dumps_to_syx(dumps: &[SysexDump]) -> Vec<u8> {
    let mut out = Vec::new();
    for d in dumps {
        out.extend_from_slice(d.bytes());
    }
    out
}

/// Parse hex text (`F0 7E … F7`), ignoring comments (`#` or `;` to end of line).
pub fn dumps_from_hex(text: &str) -> Result<Vec<SysexDump>, SysexError> {
    let mut bytes = Vec::new();
    for line in text.lines() {
        let line = line.split(['#', ';']).next().unwrap_or(line);
        for token in line.split(|c: char| c.is_ascii_whitespace() || c == ',') {
            let token = token
                .trim()
                .trim_start_matches("0x")
                .trim_start_matches("0X");
            if token.is_empty() {
                continue;
            }
            let b =
                u8::from_str_radix(token, 16).map_err(|_| SysexError::Hex(token.to_string()))?;
            bytes.push(b);
        }
    }
    dumps_from_syx(&bytes)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityReply {
    pub device: u8,
    pub manufacturer: Vec<u8>,
    pub family: u16,
    pub member: u16,
    pub software: [u8; 4],
}

impl IdentityReply {
    pub fn manufacturer_label(&self) -> String {
        manufacturer_name(&self.manufacturer).map_or_else(
            || {
                self.manufacturer
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            },
            |n| n.to_string(),
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "{mfr}  ch/dev {device:02X}  family {family:04X}  member {member:04X}  sw {sw}",
            mfr = self.manufacturer_label(),
            device = self.device,
            family = self.family,
            member = self.member,
            sw = self
                .software
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(".")
        )
    }

    pub fn file_stem(&self) -> String {
        let mfr = self
            .manufacturer_label()
            .to_lowercase()
            .replace([' ', '/'], "-");
        format!("{mfr}-{family:04x}-{member:04x}", family = self.family, member = self.member)
    }
}

/// Line-oriented hex dump of bytes that differ between `a` and `b`.
pub fn hex_diff(a: &[u8], b: &[u8]) -> String {
    if a == b {
        return format!("identical ({} bytes)", a.len());
    }
    let mut out = format!("A {} B {} bytes\n", a.len(), b.len());
    let n = a.len().max(b.len());
    let mut i = 0;
    while i < n {
        let end = (i + 16).min(n);
        let row_a = a.get(i..end.min(a.len())).unwrap_or(&[]);
        let row_b = b.get(i..end.min(b.len())).unwrap_or(&[]);
        if row_a != row_b {
            out.push_str(&format!(
                "{i:04X}  {:<47} |  {:<47}\n",
                hex_row(a, i, end),
                hex_row(b, i, end)
            ));
        }
        i = end;
    }
    out
}

fn hex_row(bytes: &[u8], start: usize, end: usize) -> String {
    let mut s = String::new();
    for i in start..end {
        if i != start {
            s.push(' ');
        }
        match bytes.get(i) {
            Some(b) => s.push_str(&format!("{b:02X}")),
            None => s.push_str("  "),
        }
    }
    s
}

pub fn parse_identity_reply(dump: &SysexDump) -> Option<IdentityReply> {
    let p = dump.payload();
    // 7E nn 06 02 mm …
    if p.len() < 11 || p[0] != 0x7E || p[2] != 0x06 || p[3] != 0x02 {
        return None;
    }
    let device = p[1];
    let (manufacturer, rest) = if p[4] == 0x00 {
        if p.len() < 13 {
            return None;
        }
        (p[4..7].to_vec(), &p[7..])
    } else {
        (vec![p[4]], &p[5..])
    };
    if rest.len() < 8 {
        return None;
    }
    Some(IdentityReply {
        device,
        manufacturer,
        family: u16::from(rest[0]) | (u16::from(rest[1]) << 8),
        member: u16::from(rest[2]) | (u16::from(rest[3]) << 8),
        software: [rest[4], rest[5], rest[6], rest[7]],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi1::Midi1Parser;

    #[test]
    fn identity_request_is_valid_dump() {
        let dump = SysexDump::identity_request();
        assert_eq!(dump.bytes(), &IDENTITY_REQUEST);
        assert_eq!(dump.to_hex(), "F0 7E 7F 06 01 F7");
    }

    #[test]
    fn assemble_complete_ump_packet() {
        let packets = SysexDump::identity_request().to_ump_packets(0);
        assert_eq!(packets.len(), 1);
        let mut asm = SysexAssembler::new();
        let out = asm.push(&packets[0]).unwrap();
        assert_eq!(out.bytes(), &IDENTITY_REQUEST);
    }

    #[test]
    fn assemble_start_then_end() {
        let mut parser = Midi1Parser::new();
        let packets = parser.push_slice(&[0xF0, 1, 2, 3, 4, 5, 6, 7, 8, 0xF7]);
        assert_eq!(packets.len(), 2);
        let mut asm = SysexAssembler::new();
        assert!(asm.push(&packets[0]).is_none());
        let dump = asm.push(&packets[1]).unwrap();
        assert_eq!(dump.bytes(), &[0xF0, 1, 2, 3, 4, 5, 6, 7, 8, 0xF7]);
    }

    #[test]
    fn syx_roundtrip_two_dumps() {
        let a = SysexDump::identity_request();
        let b = SysexDump::from_bytes(vec![0xF0, 0x41, 0x10, 0xF7]).unwrap();
        let blob = dumps_to_syx(&[a.clone(), b.clone()]);
        let parsed = dumps_from_syx(&blob).unwrap();
        assert_eq!(parsed, vec![a, b]);
    }

    #[test]
    fn hex_parse_ignores_comments() {
        let dumps = dumps_from_hex("# identity\nF0 7E 7F 06 01 F7 ; end\n").unwrap();
        assert_eq!(dumps[0].bytes(), &IDENTITY_REQUEST);
    }

    #[test]
    fn roland_checksum_replaces_last_payload_byte() {
        let dump = SysexDump::from_bytes(vec![0xF0, 0x41, 0x10, 0x42, 0x00, 0xF7]).unwrap();
        let fixed = dump.with_roland_checksum().unwrap();
        let payload = fixed.payload();
        let sum: u32 = payload[..payload.len() - 1]
            .iter()
            .map(|&b| u32::from(b))
            .sum();
        assert_eq!(payload[payload.len() - 1], roland_checksum_from_sum(sum));
        assert_eq!(*fixed.bytes().last().unwrap(), 0xF7);
    }

    #[test]
    fn identity_reply_decode() {
        // 7E 01 06 02 43 00 01 00 02 01 02 03 04
        let dump = SysexDump::from_bytes(vec![
            0xF0, 0x7E, 0x01, 0x06, 0x02, 0x43, 0x00, 0x01, 0x00, 0x02, 0x01, 0x02, 0x03, 0x04,
            0xF7,
        ])
        .unwrap();
        let id = parse_identity_reply(&dump).unwrap();
        assert_eq!(id.device, 0x01);
        assert_eq!(id.manufacturer, vec![0x43]);
        assert_eq!(id.family, 0x0100);
        assert_eq!(id.member, 0x0200);
        assert_eq!(id.software, [1, 2, 3, 4]);
        assert_eq!(id.manufacturer_label(), "Yamaha");
        assert!(id.summary().contains("Yamaha"));
    }

    #[test]
    fn hex_diff_reports_changed_row() {
        let a = [0xF0, 0x41, 0x10, 0xF7];
        let b = [0xF0, 0x41, 0x11, 0xF7];
        let d = hex_diff(&a, &b);
        assert!(d.contains("10"));
        assert!(d.contains("11"));
        assert_eq!(hex_diff(&a, &a), "identical (4 bytes)");
    }

    #[test]
    fn rejects_unframed() {
        assert!(SysexDump::from_bytes(vec![0x7E, 0xF7]).is_err());
        assert!(dumps_from_syx(&[0x90, 0x3C, 0x7F]).is_err());
    }
}
