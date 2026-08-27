//! Network MIDI 2.0 UDP session commands (M2-124 subset).
//!
//! Command packets are 32-bit words, big-endian on the wire. UMP data packets
//! are a sequence of UMP words. Auth modes are not implemented.

use crate::ump::UmpMessage;

pub const DEFAULT_PORT: u16 = 5004;

pub const CMD_INVITATION: u8 = 0x01;
pub const CMD_INVITATION_ACCEPT: u8 = 0x10;
pub const CMD_PING: u8 = 0x21;

/// Command packet: word0 = (payload_words << 16) | (command << 8) | flags.
pub fn encode_command(command: u8, flags: u8, payload: &[u32]) -> Vec<u8> {
    let mut words = Vec::with_capacity(1 + payload.len());
    let header = ((payload.len() as u32) << 16) | (u32::from(command) << 8) | u32::from(flags);
    words.push(header);
    words.extend_from_slice(payload);
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_be_bytes());
    }
    out
}

pub fn decode_command(bytes: &[u8]) -> Option<(u8, u8, Vec<u32>)> {
    if bytes.len() < 4 || bytes.len() % 4 != 0 {
        return None;
    }
    let header = u32::from_be_bytes(bytes[0..4].try_into().ok()?);
    let n = (header >> 16) as usize;
    let command = ((header >> 8) & 0xFF) as u8;
    let flags = (header & 0xFF) as u8;
    if bytes.len() != (1 + n) * 4 {
        return None;
    }
    let mut payload = Vec::with_capacity(n);
    for i in 0..n {
        let o = 4 + i * 4;
        payload.push(u32::from_be_bytes(bytes[o..o + 4].try_into().ok()?));
    }
    Some((command, flags, payload))
}

pub fn invitation(ep_name: &str) -> Vec<u8> {
    let mut name = ep_name.as_bytes().to_vec();
    name.truncate(32);
    while name.len() % 4 != 0 {
        name.push(0);
    }
    let mut payload = Vec::new();
    for c in name.chunks_exact(4) {
        payload.push(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    encode_command(CMD_INVITATION, 0, &payload)
}

pub fn encode_ump(packets: &[UmpMessage]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in packets {
        for w in p.words() {
            out.extend_from_slice(&w.to_be_bytes());
        }
    }
    out
}

pub fn decode_ump(bytes: &[u8]) -> Vec<UmpMessage> {
    if bytes.len() % 4 != 0 {
        return Vec::new();
    }
    let mut words = Vec::new();
    for c in bytes.chunks_exact(4) {
        words.push(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        match UmpMessage::try_from_words(&words[i..]) {
            Ok(msg) => {
                i += msg.len();
                out.push(msg);
            }
            Err(_) => break,
        }
    }
    out
}

pub fn looks_like_command(bytes: &[u8]) -> bool {
    decode_command(bytes)
        .is_some_and(|(c, _, _)| c == CMD_INVITATION || c == CMD_PING || c == CMD_INVITATION_ACCEPT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitation_roundtrip() {
        let b = invitation("Forge");
        let (cmd, _, payload) = decode_command(&b).unwrap();
        assert_eq!(cmd, CMD_INVITATION);
        assert!(!payload.is_empty());
    }

    #[test]
    fn ump_datagram_roundtrip() {
        let n = UmpMessage::midi1_channel_voice(0, 0x90, 60, 100);
        let b = encode_ump(&[n]);
        let back = decode_ump(&b);
        assert_eq!(back, vec![n]);
    }
}
