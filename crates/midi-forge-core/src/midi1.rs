use crate::ump::UmpMessage;

/// MIDI 1.0 bytestream parser that emits UMP packets (group 0 by default).
///
/// Handles running status, system real-time interleaved with channel data,
/// and SysEx7 chunking (6 payload bytes per UMP packet, F0/F7 stripped).
pub struct Midi1Parser {
    group: u8,
    running_status: Option<u8>,
    data: [u8; 2],
    got: u8,
    need: u8,
    sysex: Vec<u8>,
    in_sysex: bool,
}

impl Default for Midi1Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Midi1Parser {
    pub fn new() -> Self {
        Self::with_group(0)
    }

    pub fn with_group(group: u8) -> Self {
        Self {
            group: group & 0x0F,
            running_status: None,
            data: [0; 2],
            got: 0,
            need: 0,
            sysex: Vec::new(),
            in_sysex: false,
        }
    }

    pub fn push_slice(&mut self, bytes: &[u8]) -> Vec<UmpMessage> {
        let mut out = Vec::new();
        for &b in bytes {
            self.push_byte(b, &mut out);
        }
        out
    }

    pub fn push_byte(&mut self, byte: u8, out: &mut Vec<UmpMessage>) {
        if byte >= 0xF8 {
            out.push(UmpMessage::midi1_system(self.group, byte, 0, 0));
            if byte == 0xFF {
                let group = self.group;
                *self = Self::with_group(group);
            }
            return;
        }

        if self.in_sysex {
            if byte == 0xF7 {
                emit_sysex(self.group, &self.sysex, true, out);
                self.sysex.clear();
                self.in_sysex = false;
            } else if byte < 0x80 {
                self.sysex.push(byte);
            } else {
                emit_sysex(self.group, &self.sysex, true, out);
                self.sysex.clear();
                self.in_sysex = false;
                self.push_byte(byte, out);
            }
            return;
        }

        if byte == 0xF0 {
            self.running_status = None;
            self.need = 0;
            self.got = 0;
            self.sysex.clear();
            self.in_sysex = true;
            return;
        }

        if byte >= 0x80 {
            self.running_status = if byte < 0xF0 { Some(byte) } else { None };
            self.need = data_len(byte);
            self.got = 0;
            if self.need == 0 {
                out.push(system_or_channel(self.group, byte, 0, 0));
            }
            return;
        }

        let Some(status) = self.running_status else {
            return;
        };

        if self.need == 0 {
            self.need = data_len(status);
            self.got = 0;
        }
        if self.need == 0 {
            return;
        }

        self.data[usize::from(self.got)] = byte;
        self.got += 1;
        if self.got >= self.need {
            let d1 = self.data[0];
            let d2 = if self.need == 2 { self.data[1] } else { 0 };
            out.push(system_or_channel(self.group, status, d1, d2));
            self.got = 0;
            if status >= 0xF0 {
                self.running_status = None;
                self.need = 0;
            }
        }
    }
}

fn sysex_chunk_status(is_start: bool, is_end: bool) -> u8 {
    match (is_start, is_end) {
        (true, true) => 0,   // complete
        (true, false) => 1,  // start
        (false, false) => 2, // continue
        (false, true) => 3,  // end
    }
}

fn emit_sysex(group: u8, payload: &[u8], ended: bool, out: &mut Vec<UmpMessage>) {
    if payload.is_empty() && ended {
        out.push(UmpMessage::sysex7(group, 0, &[]));
        return;
    }
    let mut offset = 0;
    let mut first = true;
    while offset < payload.len() || (ended && first && payload.is_empty()) {
        let remain = payload.len() - offset;
        let take = remain.min(6);
        let last = ended && offset + take == payload.len();
        let status = sysex_chunk_status(first, last);
        out.push(UmpMessage::sysex7(
            group,
            status,
            &payload[offset..offset + take],
        ));
        offset += take;
        first = false;
        if take == 0 {
            break;
        }
    }
}

fn data_len(status: u8) -> u8 {
    match status {
        0x80..=0xBF | 0xE0..=0xEF | 0xF2 => 2,
        0xC0..=0xDF | 0xF1 | 0xF3 => 1,
        _ => 0,
    }
}

fn system_or_channel(group: u8, status: u8, d1: u8, d2: u8) -> UmpMessage {
    if status >= 0xF0 {
        UmpMessage::midi1_system(group, status, d1, d2)
    } else {
        UmpMessage::midi1_channel_voice(group, status, d1, d2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_three_bytes() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[0x90, 60, 127]);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].words()[0], 0x2090_3C7F);
    }

    #[test]
    fn running_status_second_note() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[0x90, 60, 127, 64, 100]);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].words()[0], 0x2090_3C7F);
        assert_eq!(msgs[1].words()[0], 0x2090_4064);
    }

    #[test]
    fn clock_between_note_data_bytes() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[0x90, 60, 0xF8, 127]);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].words()[0], 0x10F8_0000);
        assert_eq!(msgs[1].words()[0], 0x2090_3C7F);
    }

    #[test]
    fn identity_request_is_complete_sysex7() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].message_type(), 0x3);
        assert_eq!(msgs[0].words()[0], 0x3004_7E7F);
        assert_eq!(msgs[0].words()[1], 0x0601_0000);
    }

    #[test]
    fn long_sysex_chunks_start_then_end() {
        let mut p = Midi1Parser::new();
        // 8 payload bytes → start(6) + end(2)
        let msgs = p.push_slice(&[0xF0, 1, 2, 3, 4, 5, 6, 7, 8, 0xF7]);
        assert_eq!(msgs.len(), 2);
        assert_eq!((msgs[0].words()[0] >> 20) & 0xF, 1); // start
        assert_eq!((msgs[0].words()[0] >> 16) & 0xF, 6);
        assert_eq!((msgs[1].words()[0] >> 20) & 0xF, 3); // end
        assert_eq!((msgs[1].words()[0] >> 16) & 0xF, 2);
    }

    #[test]
    fn program_change_one_data_byte() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[0xC3, 12]);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].words()[0], 0x20C3_0C00);
    }

    #[test]
    fn stray_data_bytes_are_ignored() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[60, 127]);
        assert!(msgs.is_empty());
    }
}
