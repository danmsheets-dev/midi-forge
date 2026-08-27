use crate::midi2::{pitch32_to_14, value32_to_7, velocity7_to_16, velocity16_to_7};
use crate::ump::UmpMessage;

#[derive(Clone, Copy, Debug, Default)]
pub struct LiveChannel {
    pub last_note: Option<u8>,
    pub last_vel: u8,
    /// Native MIDI 2 velocity (MIDI 1 notes are upscaled).
    pub last_vel16: u16,
    pub sounding: u8,
    pub program: u8,
    pub pressure: u8,
    pub bend: u16,
    pub last_cc: Option<(u8, u8)>,
    /// MIDI 2 CC controller + 32-bit value.
    pub last_cc32: Option<(u8, u32)>,
    /// Last per-note pitch bend: (note, 32-bit value).
    pub pn_bend: Option<(u8, u32)>,
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
        match packet.message_type() {
            0x4 => self.push_midi2(packet),
            0x2 => self.push_midi1(packet),
            _ => {}
        }
    }

    fn push_midi2(&mut self, packet: &UmpMessage) {
        let status = packet.status_byte();
        let i = usize::from(status & 0x0F);
        let d1 = packet.data1();
        let w1 = packet.words().get(1).copied().unwrap_or(0);
        let c = &mut self.ch[i];
        match status & 0xF0 {
            0x80 => {
                c.last_vel16 = (w1 >> 16) as u16;
                c.last_vel = velocity16_to_7(c.last_vel16);
                c.sounding = c.sounding.saturating_sub(1);
                c.dirty = true;
            }
            0x90 => {
                // MIDI 2 Note On velocity 0 is still Note On (M2-104-UM D.2.1).
                c.last_note = Some(d1);
                c.last_vel16 = (w1 >> 16) as u16;
                c.last_vel = velocity16_to_7(c.last_vel16);
                c.sounding = c.sounding.saturating_add(1).min(32);
                c.dirty = true;
            }
            0xB0 => {
                c.last_cc = Some((d1, value32_to_7(w1)));
                c.last_cc32 = Some((d1, w1));
                if d1 == 123 || d1 == 120 {
                    c.sounding = 0;
                }
                c.dirty = true;
            }
            0xC0 => {
                c.program = ((w1 >> 24) & 0x7F) as u8;
                c.dirty = true;
            }
            0xD0 => {
                c.pressure = value32_to_7(w1);
                c.dirty = true;
            }
            0xE0 => {
                let (lsb, msb) = pitch32_to_14(w1);
                c.bend = u16::from(lsb) | (u16::from(msb) << 7);
                c.dirty = true;
            }
            0x60 => {
                c.pn_bend = Some((d1, w1));
                c.dirty = true;
            }
            _ => {}
        }
    }

    fn push_midi1(&mut self, packet: &UmpMessage) {
        let status = packet.status_byte();
        let i = usize::from(status & 0x0F);
        let d1 = packet.data1();
        let d2 = packet.data2();
        let c = &mut self.ch[i];
        match status & 0xF0 {
            0x80 => {
                c.last_vel = d2;
                c.last_vel16 = velocity7_to_16(d2);
                c.sounding = c.sounding.saturating_sub(1);
                c.dirty = true;
            }
            0x90 => {
                if d2 == 0 {
                    c.sounding = c.sounding.saturating_sub(1);
                    c.last_vel = 0;
                    c.last_vel16 = 0;
                } else {
                    c.last_note = Some(d1);
                    c.last_vel = d2;
                    c.last_vel16 = velocity7_to_16(d2);
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

    pub fn dump(&self) -> String {
        let mut s = String::from("Live\n");
        for (i, ch) in self.ch.iter().enumerate() {
            if !ch.dirty && ch.sounding == 0 && ch.last_cc.is_none() {
                continue;
            }
            let note = ch
                .last_note
                .map(|n| n.to_string())
                .unwrap_or_else(|| "—".into());
            let cc = ch
                .last_cc
                .map(|(n, v)| format!("{}={v}", crate::cc::cc_label(n)))
                .unwrap_or_else(|| "—".into());
            let vel = if ch.last_vel16 != 0 {
                format!(" vel16 {}", ch.last_vel16)
            } else {
                String::new()
            };
            let pn = ch
                .pn_bend
                .map(|(n, v)| format!(" pn {n} {v:#x}"))
                .unwrap_or_default();
            s.push_str(&format!(
                "  Ch{} note {note} n={} prog {} {cc} bend {}{vel}{pn}\n",
                i + 1,
                ch.sounding,
                ch.program,
                ch.bend
            ));
        }
        if s == "Live\n" {
            s.push_str("  (silent)\n");
        }
        s
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
        // Native type 0x4 — LiveView must not require a type 0x2 projection.
        let mut v = LiveView::new();
        let pkt = UmpMessage::midi2_channel_voice(0, 0x94, 48, 0, 0x8000_0000);
        assert_eq!(pkt.message_type(), 0x4);
        v.push(&pkt);
        assert_eq!(v.ch[4].last_note, Some(48));
        assert_eq!(v.ch[4].sounding, 1);
        assert_eq!(v.ch[4].last_vel16, 0x8000);
        assert_eq!(v.ch[4].last_vel, crate::midi2::velocity16_to_7(0x8000));
    }

    #[test]
    fn midi2_note_on_zero_velocity_still_sounds() {
        let mut v = LiveView::new();
        v.push(&UmpMessage::midi2_channel_voice(0, 0x90, 60, 0, 0));
        assert_eq!(v.ch[0].last_note, Some(60));
        assert_eq!(v.ch[0].sounding, 1);
        assert_eq!(v.ch[0].last_vel16, 0);
    }

    #[test]
    fn midi2_note_off_decrements_sounding() {
        let mut v = LiveView::new();
        v.push(&crate::midi2::midi2_note_on(0, 2, 64, 0xFFFF));
        v.push(&crate::midi2::midi2_note_off(0, 2, 64, 0x1000));
        assert_eq!(v.ch[2].sounding, 0);
        assert_eq!(v.ch[2].last_vel16, 0x1000);
    }

    #[test]
    fn midi2_per_note_bend_is_stored() {
        let mut v = LiveView::new();
        v.push(&crate::midi2::midi2_per_note_pitch_bend(
            0,
            1,
            60,
            0xC000_0000,
        ));
        assert_eq!(v.ch[1].pn_bend, Some((60, 0xC000_0000)));
        assert!(v.ch[1].dirty);
    }

    #[test]
    fn midi2_cc_keeps_32bit() {
        let mut v = LiveView::new();
        v.push(&crate::midi2::midi2_cc(0, 3, 7, 0x8000_0000));
        assert_eq!(v.ch[3].last_cc, Some((7, 64)));
        assert_eq!(v.ch[3].last_cc32, Some((7, 0x8000_0000)));
    }
}
