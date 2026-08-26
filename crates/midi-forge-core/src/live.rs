use crate::midi2::downscale_to_midi1;
use crate::ump::UmpMessage;

#[derive(Clone, Copy, Debug, Default)]
pub struct LiveChannel {
    pub last_note: Option<u8>,
    pub last_vel: u8,
    pub sounding: u8,
    pub program: u8,
    pub pressure: u8,
    pub bend: u16,
    pub last_cc: Option<(u8, u8)>,
    pub dirty: bool,
}

/// ShowMIDI-style “now” state per MIDI channel.
#[derive(Clone, Debug)]
pub struct LiveView {
    pub ch: [LiveChannel; 16],
}

impl Default for LiveView {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveView {
    pub fn new() -> Self {
        Self {
            ch: [LiveChannel {
                bend: 8192,
                ..LiveChannel::default()
            }; 16],
        }
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
        let i = usize::from(status & 0x0F);
        let d1 = packet.data1();
        let d2 = packet.data2();
        let c = &mut self.ch[i];
        match status & 0xF0 {
            0x80 => {
                c.sounding = c.sounding.saturating_sub(1);
                c.dirty = true;
            }
            0x90 => {
                if d2 == 0 {
                    c.sounding = c.sounding.saturating_sub(1);
                } else {
                    c.last_note = Some(d1);
                    c.last_vel = d2;
                    c.sounding = c.sounding.saturating_add(1).min(32);
                }
                c.dirty = true;
            }
            0xB0 => {
                c.last_cc = Some((d1, d2));
                if d1 == 123 || d1 == 120 {
                    c.sounding = 0;
                }
                c.dirty = true;
            }
            0xC0 => {
                c.program = d1;
                c.dirty = true;
            }
            0xD0 => {
                c.pressure = d1;
                c.dirty = true;
            }
            0xE0 => {
                c.bend = u16::from(d1) | (u16::from(d2) << 7);
                c.dirty = true;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_and_cc_update_channel() {
        let mut v = LiveView::new();
        v.push(&UmpMessage::midi1_channel_voice(0, 0x93, 60, 100));
        v.push(&UmpMessage::midi1_channel_voice(0, 0xB3, 1, 64));
        assert_eq!(v.ch[3].last_note, Some(60));
        assert_eq!(v.ch[3].sounding, 1);
        assert_eq!(v.ch[3].last_cc, Some((1, 64)));
    }

    #[test]
    fn midi2_note_downscales_into_live() {
        let mut v = LiveView::new();
        v.push(&UmpMessage::midi2_channel_voice(
            0,
            0x94,
            48,
            0,
            0x8000_0000,
        ));
        assert_eq!(v.ch[4].last_note, Some(48));
        assert_eq!(v.ch[4].sounding, 1);
    }
}
