use crate::sysex::SysexError;
use crate::ump::UmpMessage;

/// SysEx8 complete / start / continue / end (status nibble).
pub const SYSEX8_COMPLETE: u8 = 0;
pub const SYSEX8_START: u8 = 1;
pub const SYSEX8_CONTINUE: u8 = 2;
pub const SYSEX8_END: u8 = 3;

/// MixData status nibbles (M2-104-UM: 8 header, 9 payload; A/B reserved as chunks).
pub const MIXDATA_HEADER: u8 = 0x8;
pub const MIXDATA_PAYLOAD: u8 = 0x9;
pub const MIXDATA_END: u8 = 0xA;
pub const MIXDATA_COMPLETE: u8 = 0xB;

const SYSEX8_MAX_DATA: usize = 13;

/// Pack a 4-word SysEx8 UMP (type 0x5, status 0–3).
///
/// `data` is payload after the stream id (0–13 bytes). Count in the packet
/// is `data.len() + 1` (stream id plus valid data bytes).
pub fn sysex8_packet(group: u8, status: u8, stream_id: u8, data: &[u8]) -> UmpMessage {
    UmpMessage::sysex8(group, status, stream_id, data)
}

/// Pack a 4-word MixData UMP (type 0x5, status 8–B). `mds_id` is the 4-bit MDS ID.
pub fn mixdata_packet(group: u8, status: u8, mds_id: u8) -> UmpMessage {
    let word0 = (0x5 << 28)
        | (u32::from(group & 0x0F) << 24)
        | (u32::from(status & 0x0F) << 20)
        | (u32::from(mds_id & 0x0F) << 16);
    UmpMessage::try_from_words(&[word0, 0, 0, 0]).expect("UMP type 0x5 is four words")
}

/// Split a SysEx8 payload into complete / start / continue / end packets.
pub fn sysex8_packets(group: u8, stream_id: u8, data: &[u8]) -> Vec<UmpMessage> {
    if data.len() <= SYSEX8_MAX_DATA {
        return vec![sysex8_packet(group, SYSEX8_COMPLETE, stream_id, data)];
    }
    let mut out = Vec::new();
    let mut offset = 0;
    let mut first = true;
    while offset < data.len() {
        let take = (data.len() - offset).min(SYSEX8_MAX_DATA);
        let last = offset + take == data.len();
        let status = match (first, last) {
            (true, true) => SYSEX8_COMPLETE,
            (true, false) => SYSEX8_START,
            (false, false) => SYSEX8_CONTINUE,
            (false, true) => SYSEX8_END,
        };
        out.push(sysex8_packet(
            group,
            status,
            stream_id,
            &data[offset..offset + take],
        ));
        offset += take;
        first = false;
    }
    out
}

/// Reassemble SysEx8 packets into a payload (`stream id` stripped, data concatenated).
#[derive(Default)]
pub struct Sysex8Assembler {
    buf: Vec<u8>,
    open: bool,
    stream_id: u8,
}

impl Sysex8Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.buf.clear();
        self.open = false;
        self.stream_id = 0;
    }

    /// Feed one UMP. `Ok(Some(payload))` when a message finishes; `Ok(None)`
    /// while assembling or if the packet is not SysEx8. Truncated streams
    /// error like a framed SysEx dump (`SysexError::Framing`) and reset.
    pub fn push(&mut self, packet: &UmpMessage) -> Result<Option<Vec<u8>>, SysexError> {
        let Some((status, count, stream_id, data)) = packet.sysex8_parts() else {
            return Ok(None);
        };
        let n = usize::from(count.saturating_sub(1).min(SYSEX8_MAX_DATA as u8));
        let payload = &data[..n];
        match status {
            SYSEX8_COMPLETE => {
                self.reset();
                Ok(Some(payload.to_vec()))
            }
            SYSEX8_START => {
                self.buf.clear();
                self.buf.extend_from_slice(payload);
                self.open = true;
                self.stream_id = stream_id;
                Ok(None)
            }
            SYSEX8_CONTINUE if self.open && self.stream_id == stream_id => {
                self.buf.extend_from_slice(payload);
                Ok(None)
            }
            SYSEX8_END if self.open && self.stream_id == stream_id => {
                self.buf.extend_from_slice(payload);
                let dump = std::mem::take(&mut self.buf);
                self.reset();
                Ok(Some(dump))
            }
            _ => {
                self.reset();
                Err(SysexError::Framing)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ump::UmpMessage;

    fn payload_20() -> Vec<u8> {
        (0u8..20).collect()
    }

    #[test]
    fn twenty_byte_payload_start_end_roundtrip() {
        let data = payload_20();
        let packets = sysex8_packets(3, 0xAB, &data);
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].len(), 4);
        assert_eq!(packets[0].message_type(), 0x5);
        assert_eq!(packets[0].group(), 3);
        assert_eq!((packets[0].words()[0] >> 20) & 0xF, u32::from(SYSEX8_START));
        assert_eq!((packets[1].words()[0] >> 20) & 0xF, u32::from(SYSEX8_END));
        // count includes stream id: 14 then 8 (13 + 7 data bytes).
        assert_eq!((packets[0].words()[0] >> 16) & 0xF, 14);
        assert_eq!((packets[1].words()[0] >> 16) & 0xF, 8);

        let mut asm = Sysex8Assembler::new();
        assert_eq!(asm.push(&packets[0]), Ok(None));
        let dump = asm.push(&packets[1]).unwrap().expect("assembled dump");
        assert_eq!(dump, data);
    }

    #[test]
    fn complete_packet_is_one_dump() {
        let msg = sysex8_packet(0, SYSEX8_COMPLETE, 7, &[0xF0, 0x01, 0x02]);
        let mut asm = Sysex8Assembler::new();
        assert_eq!(asm.push(&msg), Ok(Some(vec![0xF0, 0x01, 0x02])));
    }

    #[test]
    fn count_one_is_stream_id_only() {
        let msg = sysex8_packet(1, SYSEX8_COMPLETE, 0x42, &[]);
        assert_eq!((msg.words()[0] >> 16) & 0xF, 1);
        assert_eq!((msg.words()[0] >> 8) & 0xFF, 0x42);
        let mut asm = Sysex8Assembler::new();
        assert_eq!(asm.push(&msg), Ok(Some(vec![])));
    }

    #[test]
    fn truncated_stream_resets_like_sysex7() {
        let mut asm = Sysex8Assembler::new();
        let start = sysex8_packet(0, SYSEX8_START, 7, &[1, 2, 3, 4, 5]);
        assert_eq!(asm.push(&start), Ok(None));
        // Complete interrupts the open start: truncated bytes are dropped.
        let complete = sysex8_packet(0, SYSEX8_COMPLETE, 7, &[10, 11]);
        assert_eq!(asm.push(&complete), Ok(Some(vec![10, 11])));

        // End without start errors and leaves the assembler idle.
        let end = sysex8_packet(0, SYSEX8_END, 1, &[1]);
        assert_eq!(asm.push(&end), Err(SysexError::Framing));
        let complete = sysex8_packet(0, SYSEX8_COMPLETE, 1, &[9]);
        assert_eq!(asm.push(&complete), Ok(Some(vec![9])));
    }

    #[test]
    fn start_continue_end_thirty_bytes() {
        let data: Vec<u8> = (0u8..30).collect();
        let packets = sysex8_packets(0, 1, &data);
        assert_eq!(packets.len(), 3);
        let statuses: Vec<u32> = packets.iter().map(|p| (p.words()[0] >> 20) & 0xF).collect();
        assert_eq!(
            statuses,
            vec![
                u32::from(SYSEX8_START),
                u32::from(SYSEX8_CONTINUE),
                u32::from(SYSEX8_END)
            ]
        );
        let mut asm = Sysex8Assembler::new();
        assert_eq!(asm.push(&packets[0]), Ok(None));
        assert_eq!(asm.push(&packets[1]), Ok(None));
        assert_eq!(asm.push(&packets[2]), Ok(Some(data)));
    }

    #[test]
    fn packet_layout_matches_ump_spec() {
        let msg = sysex8_packet(0, SYSEX8_START, 0xAB, &(0u8..13).collect::<Vec<_>>());
        assert_eq!(
            msg.words(),
            &[0x501E_AB00, 0x0102_0304, 0x0506_0708, 0x090A_0B0C]
        );
        let complete = sysex8_packet(2, SYSEX8_COMPLETE, 1, &[0x99]);
        assert_eq!(complete.words()[0], 0x5202_0199);
        assert_eq!(UmpMessage::sysex8(2, SYSEX8_COMPLETE, 1, &[0x99]), complete);
    }

    #[test]
    fn mixdata_header_layout() {
        let msg = mixdata_packet(2, MIXDATA_HEADER, 5);
        assert_eq!(msg.len(), 4);
        assert_eq!(msg.message_type(), 0x5);
        assert_eq!(msg.group(), 2);
        assert_eq!(msg.words()[0], 0x5285_0000);
    }

    #[test]
    fn assembler_ignores_non_sysex8() {
        let mut asm = Sysex8Assembler::new();
        let note = UmpMessage::midi1_channel_voice(0, 0x90, 60, 127);
        assert_eq!(asm.push(&note), Ok(None));
        let mds = mixdata_packet(0, MIXDATA_HEADER, 1);
        assert_eq!(asm.push(&mds), Ok(None));
    }
}
