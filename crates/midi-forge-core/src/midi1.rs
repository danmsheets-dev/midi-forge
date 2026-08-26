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

/// Data bytes that follow a MIDI 1.0 status byte (0, 1, or 2).
pub fn midi1_data_len(status: u8) -> u8 {
    data_len(status)
}

/// Convert a complete MIDI 1.0 message into UMP (group 0).
pub fn ump_from_status_data(status: u8, data1: u8, data2: u8) -> UmpMessage {
    system_or_channel(0, status, data1, data2)
}

/// WinMM `MIM_DATA` packing: `status | data1 << 8 | data2 << 16`.
pub fn ump_from_packed_short(packed: u32) -> UmpMessage {
    let status = (packed & 0xFF) as u8;
    let data1 = ((packed >> 8) & 0xFF) as u8;
    let data2 = ((packed >> 16) & 0xFF) as u8;
    ump_from_status_data(status, data1, data2)
}

/// Pack a MIDI 1.0 UMP channel/system message for `midiOutShortMsg`.
pub fn packed_short_from_ump(msg: &UmpMessage) -> Option<u32> {
    match msg.message_type() {
        0x1 | 0x2 => {
            let word = msg.words()[0];
            let status = ((word >> 16) & 0xFF) as u8;
            let data1 = ((word >> 8) & 0xFF) as u8;
            let data2 = (word & 0xFF) as u8;
            Some(u32::from(status) | (u32::from(data1) << 8) | (u32::from(data2) << 16))
        }
        _ => None,
    }
}

/// Hex of the original MIDI 1.0 bytes, or raw UMP words for other types.
pub fn format_wire_hex(msg: &UmpMessage) -> String {
    match msg.message_type() {
        0x1 | 0x2 => {
            let word = msg.words()[0];
            let status = ((word >> 16) & 0xFF) as u8;
            let data1 = ((word >> 8) & 0xFF) as u8;
            let data2 = (word & 0xFF) as u8;
            match data_len(status) {
                0 => format!("{status:02X}"),
                1 => format!("{status:02X} {data1:02X}"),
                _ => format!("{status:02X} {data1:02X} {data2:02X}"),
            }
        }
        0x3 => {
            let w0 = msg.words()[0];
            let w1 = msg.words().get(1).copied().unwrap_or(0);
            let status = ((w0 >> 20) & 0xF) as u8;
            let count = ((w0 >> 16) & 0xF) as usize;
            let bytes = [
                ((w0 >> 8) & 0xFF) as u8,
                (w0 & 0xFF) as u8,
                ((w1 >> 24) & 0xFF) as u8,
                ((w1 >> 16) & 0xFF) as u8,
                ((w1 >> 8) & 0xFF) as u8,
                (w1 & 0xFF) as u8,
            ];
            let payload = bytes
                .iter()
                .take(count.min(6))
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join(" ");
            match status {
                0 if payload.is_empty() => "F0 F7".into(),
                0 => format!("F0 {payload} F7"),
                1 => format!("F0 {payload} …"),
                2 => format!("… {payload} …"),
                3 => format!("… {payload} F7"),
                _ => format!("SysEx7 {payload}"),
            }
        }
        _ => msg
            .words()
            .iter()
            .map(|w| format!("{w:08X}"))
            .collect::<Vec<_>>()
            .join(" "),
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

    #[test]
    fn packed_winmm_note_on_matches_ump() {
        let packed = 0x90 | (60 << 8) | (127 << 16);
        let msg = ump_from_packed_short(packed);
        assert_eq!(msg.words()[0], 0x2090_3C7F);
        assert_eq!(packed_short_from_ump(&msg), Some(packed));
        assert_eq!(format_wire_hex(&msg), "90 3C 7F");
    }

    #[test]
    fn packed_clock_is_system_ump() {
        let msg = ump_from_packed_short(0xF8);
        assert_eq!(msg.message_type(), 0x1);
        assert_eq!(msg.words()[0], 0x10F8_0000);
        assert_eq!(format_wire_hex(&msg), "F8");
    }

    #[test]
    fn program_change_hex_omits_second_data_byte() {
        let msg = ump_from_status_data(0xC3, 12, 0);
        assert_eq!(format_wire_hex(&msg), "C3 0C");
    }

    #[test]
    fn sysex_hex_does_not_fake_f7_on_start_chunk() {
        let start = UmpMessage::sysex7(0, 1, &[1, 2, 3, 4, 5, 6]);
        let end = UmpMessage::sysex7(0, 3, &[7, 8]);
        let complete = UmpMessage::sysex7(0, 0, &[0x7E, 0x7F, 0x06, 0x01]);
        assert_eq!(format_wire_hex(&start), "F0 01 02 03 04 05 06 …");
        assert_eq!(format_wire_hex(&end), "… 07 08 F7");
        assert_eq!(format_wire_hex(&complete), "F0 7E 7F 06 01 F7");
    }
}
