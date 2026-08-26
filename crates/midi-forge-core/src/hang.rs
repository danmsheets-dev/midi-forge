use std::collections::BTreeSet;

use crate::midi2::downscale_to_midi1;
use crate::ump::UmpMessage;

const CC_ALL_SOUND_OFF: u8 = 120;
const CC_ALL_NOTES_OFF: u8 = 123;

/// A note that has been seen NoteOn (vel > 0) without a matching NoteOff.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct HangNote {
    pub channel: u8,
    pub note: u8,
}

/// Tracks sounding notes for the stuck-note panel and targeted panic.
#[derive(Clone, Debug, Default)]
pub struct HangTracker {
    notes: BTreeSet<(u8, u8)>,
}

impl HangTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notes(&self) -> Vec<HangNote> {
        self.notes
            .iter()
            .map(|&(channel, note)| HangNote { channel, note })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn clear(&mut self) {
        self.notes.clear();
    }

    pub fn push(&mut self, packet: &UmpMessage) {
        if packet.message_type() == 0x4 {
            for p in downscale_to_midi1(packet) {
                self.push_midi1(&p);
            }
            return;
        }
        self.push_midi1(packet);
    }

    fn push_midi1(&mut self, packet: &UmpMessage) {
        if packet.message_type() != 0x2 {
            return;
        }
        let status = packet.status_byte();
        let ch = status & 0x0F;
        let d1 = packet.data1();
        let d2 = packet.data2();
        match status & 0xF0 {
            0x80 => {
                self.notes.remove(&(ch, d1 & 0x7F));
            }
            0x90 => {
                let note = d1 & 0x7F;
                if d2 == 0 {
                    self.notes.remove(&(ch, note));
                } else {
                    self.notes.insert((ch, note));
                    if self.notes.len() > 256
                        && let Some(first) = self.notes.iter().copied().next()
                    {
                        self.notes.remove(&first);
                    }
                }
            }
            0xB0 if d1 == CC_ALL_NOTES_OFF || d1 == CC_ALL_SOUND_OFF => {
                self.notes.retain(|&(c, _)| c != ch);
            }
            _ => {}
        }
    }

    /// Note-off packets for every hanging note (vel 0).
    pub fn note_off_packets(&self) -> Vec<UmpMessage> {
        self.notes
            .iter()
            .map(|&(ch, note)| UmpMessage::midi1_channel_voice(0, 0x80 | ch, note, 0))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(ch: u8, note: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x90 | ch, note, 100)
    }

    fn off(ch: u8, note: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x80 | ch, note, 0)
    }

    #[test]
    fn note_on_then_off() {
        let mut h = HangTracker::new();
        h.push(&on(0, 60));
        h.push(&on(0, 64));
        assert_eq!(h.len(), 2);
        h.push(&off(0, 60));
        assert_eq!(
            h.notes(),
            vec![HangNote {
                channel: 0,
                note: 64
            }]
        );
    }

    #[test]
    fn vel_zero_is_off() {
        let mut h = HangTracker::new();
        h.push(&on(1, 10));
        h.push(&UmpMessage::midi1_channel_voice(0, 0x91, 10, 0));
        assert!(h.is_empty());
    }

    #[test]
    fn all_notes_off_clears_channel() {
        let mut h = HangTracker::new();
        h.push(&on(2, 1));
        h.push(&on(3, 2));
        h.push(&UmpMessage::midi1_channel_voice(0, 0xB2, 123, 0));
        assert_eq!(
            h.notes(),
            vec![HangNote {
                channel: 3,
                note: 2
            }]
        );
    }

    #[test]
    fn note_off_packets_match_hangs() {
        let mut h = HangTracker::new();
        h.push(&on(4, 70));
        let pk = h.note_off_packets();
        assert_eq!(pk.len(), 1);
        assert_eq!(pk[0].status_byte(), 0x84);
        assert_eq!(pk[0].data1(), 70);
    }
}
