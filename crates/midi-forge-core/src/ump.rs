use crate::error::CoreError;

/// Universal MIDI Packet: 1, 2, or 4 32-bit words.
///
/// Word count is determined by message type (bits 31–28 of word 0), matching
/// the MIDI 2.0 UMP spec. Invalid lengths cannot be constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct UmpMessage {
    words: [u32; 4],
    len: u8,
}

impl UmpMessage {
    /// Number of 32-bit words for a UMP message type, if the type is defined.
    pub const fn word_count(message_type: u8) -> Option<usize> {
        match message_type & 0x0F {
            0x0..=0x2 | 0x6 | 0x7 => Some(1),
            0x3 | 0x4 | 0x8..=0xA => Some(2),
            0xB | 0xC => Some(3),
            0x5 | 0xD | 0xE | 0xF => Some(4),
            _ => None,
        }
    }

    pub fn try_from_words(words: &[u32]) -> Result<Self, CoreError> {
        if words.is_empty() {
            return Err(CoreError::EmptyPacket);
        }
        let mt = ((words[0] >> 28) & 0xF) as u8;
        let needed = Self::word_count(mt).ok_or(CoreError::UnknownMessageType(mt))?;
        if words.len() < needed {
            return Err(CoreError::WrongWordCount {
                needed,
                got: words.len(),
            });
        }
        let mut buf = [0u32; 4];
        buf[..needed].copy_from_slice(&words[..needed]);
        Ok(Self {
            words: buf,
            len: needed as u8,
        })
    }

    pub fn from_word(word: u32) -> Result<Self, CoreError> {
        Self::try_from_words(&[word])
    }

    /// MIDI 2.0 Channel Voice (UMP type 0x4), two words.
    pub fn midi2_channel_voice(group: u8, status: u8, data1: u8, data2: u8, word1: u32) -> Self {
        let word0 = (0x4 << 28)
            | (u32::from(group & 0x0F) << 24)
            | (u32::from(status) << 16)
            | (u32::from(data1) << 8)
            | u32::from(data2);
        Self {
            words: [word0, word1, 0, 0],
            len: 2,
        }
    }

    /// MIDI 1.0 Channel Voice (UMP type 0x2), one word.
    pub fn midi1_channel_voice(group: u8, status: u8, data1: u8, data2: u8) -> Self {
        let word = (0x2 << 28)
            | (u32::from(group & 0x0F) << 24)
            | (u32::from(status) << 16)
            | (u32::from(data1) << 8)
            | u32::from(data2);
        Self {
            words: [word, 0, 0, 0],
            len: 1,
        }
    }

    /// MIDI 1.0 System Common / Real Time (UMP type 0x1), one word.
    pub fn midi1_system(group: u8, status: u8, data1: u8, data2: u8) -> Self {
        let word = (0x1 << 28)
            | (u32::from(group & 0x0F) << 24)
            | (u32::from(status) << 16)
            | (u32::from(data1) << 8)
            | u32::from(data2);
        Self {
            words: [word, 0, 0, 0],
            len: 1,
        }
    }

    /// SysEx8 (UMP type 0x5), four words.
    ///
    /// `status`: 0 complete, 1 start, 2 continue, 3 end.
    /// `data` is 0–13 payload bytes after the stream id.
    pub fn sysex8(group: u8, status: u8, stream_id: u8, data: &[u8]) -> Self {
        let n = data.len().min(13);
        let b = |i: usize| u32::from(*data.get(i).unwrap_or(&0));
        let word0 = (0x5 << 28)
            | (u32::from(group & 0x0F) << 24)
            | (u32::from(status & 0x0F) << 20)
            | (((n as u32) + 1) << 16)
            | (u32::from(stream_id) << 8)
            | b(0);
        let word1 = (b(1) << 24) | (b(2) << 16) | (b(3) << 8) | b(4);
        let word2 = (b(5) << 24) | (b(6) << 16) | (b(7) << 8) | b(8);
        let word3 = (b(9) << 24) | (b(10) << 16) | (b(11) << 8) | b(12);
        Self {
            words: [word0, word1, word2, word3],
            len: 4,
        }
    }

    /// SysEx7 (UMP type 0x3), two words.
    ///
    /// `status`: 0 complete, 1 start, 2 continue, 3 end.
    /// `data` is 0–6 payload bytes (F0/F7 stripped).
    pub fn sysex7(group: u8, status: u8, data: &[u8]) -> Self {
        let n = data.len().min(6);
        let b = |i: usize| u32::from(*data.get(i).unwrap_or(&0));
        let word0 = (0x3 << 28)
            | (u32::from(group & 0x0F) << 24)
            | (u32::from(status & 0x0F) << 20)
            | ((n as u32) << 16)
            | (b(0) << 8)
            | b(1);
        let word1 = (b(2) << 24) | (b(3) << 16) | (b(4) << 8) | b(5);
        Self {
            words: [word0, word1, 0, 0],
            len: 2,
        }
    }

    pub fn len(&self) -> usize {
        usize::from(self.len)
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn words(&self) -> &[u32] {
        &self.words[..self.len()]
    }

    pub fn message_type(&self) -> u8 {
        ((self.words[0] >> 28) & 0xF) as u8
    }

    pub fn group(&self) -> u8 {
        ((self.words[0] >> 24) & 0xF) as u8
    }

    pub fn status_byte(&self) -> u8 {
        ((self.words[0] >> 16) & 0xFF) as u8
    }

    pub fn data1(&self) -> u8 {
        ((self.words[0] >> 8) & 0xFF) as u8
    }

    pub fn data2(&self) -> u8 {
        (self.words[0] & 0xFF) as u8
    }

    /// SysEx8: (status 0–3, valid byte count including stream id, stream id, payload).
    pub fn sysex8_parts(&self) -> Option<(u8, u8, u8, [u8; 13])> {
        if self.message_type() != 0x5 {
            return None;
        }
        let w0 = self.words[0];
        let status = ((w0 >> 20) & 0xF) as u8;
        if status > 3 {
            return None;
        }
        let count = ((w0 >> 16) & 0xF) as u8;
        let stream_id = ((w0 >> 8) & 0xFF) as u8;
        let w1 = self.words[1];
        let w2 = self.words[2];
        let w3 = self.words[3];
        let data = [
            (w0 & 0xFF) as u8,
            ((w1 >> 24) & 0xFF) as u8,
            ((w1 >> 16) & 0xFF) as u8,
            ((w1 >> 8) & 0xFF) as u8,
            (w1 & 0xFF) as u8,
            ((w2 >> 24) & 0xFF) as u8,
            ((w2 >> 16) & 0xFF) as u8,
            ((w2 >> 8) & 0xFF) as u8,
            (w2 & 0xFF) as u8,
            ((w3 >> 24) & 0xFF) as u8,
            ((w3 >> 16) & 0xFF) as u8,
            ((w3 >> 8) & 0xFF) as u8,
            (w3 & 0xFF) as u8,
        ];
        Some((status, count, stream_id, data))
    }

    /// SysEx7: (status 0–3, valid byte count, payload). F0/F7 are not included.
    pub fn sysex7_parts(&self) -> Option<(u8, u8, [u8; 6])> {
        if self.message_type() != 0x3 {
            return None;
        }
        let w0 = self.words[0];
        let w1 = self.words.get(1).copied().unwrap_or(0);
        let status = ((w0 >> 20) & 0xF) as u8;
        let count = ((w0 >> 16) & 0xF) as u8;
        let data = [
            ((w0 >> 8) & 0xFF) as u8,
            (w0 & 0xFF) as u8,
            ((w1 >> 24) & 0xFF) as u8,
            ((w1 >> 16) & 0xFF) as u8,
            ((w1 >> 8) & 0xFF) as u8,
            (w1 & 0xFF) as u8,
        ];
        Some((status, count, data))
    }

    /// MIDI channel (0–15) for MIDI 1.0 or MIDI 2.0 channel-voice packets.
    pub fn channel(&self) -> Option<u8> {
        matches!(self.message_type(), 0x2 | 0x4).then_some(self.status_byte() & 0x0F)
    }

    /// Rewrite the channel nibble on MIDI 1.0 / MIDI 2.0 channel voice.
    pub fn with_channel(mut self, channel: u8) -> Self {
        if matches!(self.message_type(), 0x2 | 0x4) {
            let status = (self.status_byte() & 0xF0) | (channel & 0x0F);
            self.words[0] = (self.words[0] & 0xFF00_FFFF) | (u32::from(status) << 16);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_count_matches_ump_spec() {
        assert_eq!(UmpMessage::word_count(0x0), Some(1));
        assert_eq!(UmpMessage::word_count(0x1), Some(1));
        assert_eq!(UmpMessage::word_count(0x2), Some(1));
        assert_eq!(UmpMessage::word_count(0x3), Some(2));
        assert_eq!(UmpMessage::word_count(0x4), Some(2));
        assert_eq!(UmpMessage::word_count(0x5), Some(4));
        assert_eq!(UmpMessage::word_count(0xD), Some(4));
        assert_eq!(UmpMessage::word_count(0xF), Some(4));
        assert_eq!(UmpMessage::word_count(0x6), Some(1));
        assert_eq!(UmpMessage::word_count(0x8), Some(2));
        assert_eq!(UmpMessage::word_count(0xB), Some(3));
        assert_eq!(UmpMessage::word_count(0xE), Some(4));
    }

    #[test]
    fn midi1_note_on_is_one_word_type_2() {
        let msg = UmpMessage::midi1_channel_voice(0, 0x90, 60, 127);
        assert_eq!(msg.len(), 1);
        assert_eq!(msg.message_type(), 0x2);
        assert_eq!(msg.group(), 0);
        assert_eq!(msg.words()[0], 0x2090_3C7F);
    }

    #[test]
    fn midi1_note_on_group_3() {
        let msg = UmpMessage::midi1_channel_voice(3, 0x91, 0, 1);
        assert_eq!(msg.group(), 3);
        assert_eq!(msg.words()[0], 0x2391_0001);
    }

    #[test]
    fn rejects_short_midi2_channel_voice() {
        let err = UmpMessage::try_from_words(&[0x4090_0000]).unwrap_err();
        assert_eq!(err, CoreError::WrongWordCount { needed: 2, got: 1 });
    }

    #[test]
    fn rejects_empty_and_reserved_type() {
        assert_eq!(
            UmpMessage::try_from_words(&[]).unwrap_err(),
            CoreError::EmptyPacket
        );
        let reserved = UmpMessage::from_word(0x6000_0000).unwrap();
        assert_eq!(reserved.len(), 1);
        assert_eq!(reserved.message_type(), 0x6);
    }

    #[test]
    fn sysex7_complete_four_bytes() {
        let msg = UmpMessage::sysex7(0, 0, &[0x7E, 0x7F, 0x06, 0x01]);
        assert_eq!(msg.len(), 2);
        assert_eq!(msg.message_type(), 0x3);
        assert_eq!(msg.words()[0], 0x3004_7E7F);
        assert_eq!(msg.words()[1], 0x0601_0000);
    }

    #[test]
    fn sysex8_complete_is_four_words_type_5() {
        let msg = UmpMessage::sysex8(0, 0, 0xAB, &[0xF0, 0x01]);
        assert_eq!(msg.len(), 4);
        assert_eq!(msg.message_type(), 0x5);
        assert_eq!(msg.words()[0], 0x5003_ABF0);
        let mut data = [0u8; 13];
        data[0] = 0xF0;
        data[1] = 0x01;
        assert_eq!(msg.sysex8_parts(), Some((0, 3, 0xAB, data)));
    }

    #[test]
    fn with_channel_rewrites_status_nibble() {
        let msg = UmpMessage::midi1_channel_voice(0, 0x90, 60, 127).with_channel(3);
        assert_eq!(msg.channel(), Some(3));
        assert_eq!(msg.status_byte(), 0x93);
        assert_eq!(msg.data1(), 60);
        assert_eq!(msg.data2(), 127);
        assert_eq!(msg.words()[0], 0x2093_3C7F);
    }

    #[test]
    fn with_channel_ignores_clock() {
        let clock = UmpMessage::midi1_system(0, 0xF8, 0, 0);
        assert_eq!(clock.channel(), None);
        assert_eq!(clock.with_channel(5), clock);
    }

    #[test]
    fn midi2_channel_and_rewrite() {
        let msg = UmpMessage::midi2_channel_voice(1, 0x92, 64, 0, 0x8000_0000).with_channel(5);
        assert_eq!(msg.channel(), Some(5));
        assert_eq!(msg.status_byte(), 0x95);
        assert_eq!(msg.message_type(), 0x4);
        assert_eq!(msg.group(), 1);
        assert_eq!(msg.data1(), 64);
    }
}
