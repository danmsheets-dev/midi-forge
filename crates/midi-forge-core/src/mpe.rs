use crate::ump::UmpMessage;

/// MPE Configuration Message is RPN 0x0006. Data Entry MSB = member channel count.
pub const MPE_CONFIG_RPN: (u8, u8) = (0x00, 0x06);
/// Pitch bend sensitivity is RPN 0x0000. Data Entry MSB = semitones.
pub const PITCH_BEND_RANGE_RPN: (u8, u8) = (0x00, 0x00);

const CC_DATA_MSB: u8 = 6;
const CC_RPN_LSB: u8 = 100;
const CC_RPN_MSB: u8 = 101;
const CC_NRPN_LSB: u8 = 98;
const CC_NRPN_MSB: u8 = 99;
const CC_TIMBRE: u8 = 74;
const CC_ALL_SOUND_OFF: u8 = 120;
const CC_RESET: u8 = 121;
const CC_ALL_NOTES_OFF: u8 = 123;

const DEFAULT_NOTE_PB_RANGE: u8 = 48;
const DEFAULT_MASTER_PB_RANGE: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MpeZoneKind {
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MpeZone {
    pub kind: MpeZoneKind,
    pub master: u8,
    pub members: u8,
}

impl MpeZone {
    pub fn contains_member(&self, channel: u8) -> bool {
        match self.kind {
            MpeZoneKind::Lower => self.members > 0 && channel >= 1 && channel <= self.members,
            MpeZoneKind::Upper => self.members > 0 && channel >= 15 - self.members && channel <= 14,
        }
    }

    pub fn is_master(&self, channel: u8) -> bool {
        channel == self.master && self.members > 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MpeVoice {
    pub channel: u8,
    pub note: u8,
    pub velocity: u8,
    /// 14-bit pitch bend, center 8192.
    pub pitch_bend: u16,
    pub pressure: u8,
    pub timbre: u8,
}

#[derive(Clone, Copy, Debug, Default)]
struct ChannelExpr {
    pitch_bend: u16,
    pressure: u8,
    timbre: u8,
}

impl ChannelExpr {
    fn new() -> Self {
        Self {
            pitch_bend: 8192,
            pressure: 0,
            timbre: 64,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RpnCell {
    msb: Option<u8>,
    lsb: Option<u8>,
    is_rpn: bool,
}

impl Default for RpnCell {
    fn default() -> Self {
        Self {
            msb: None,
            lsb: None,
            is_rpn: true,
        }
    }
}

/// Live MPE zone layout and sounding notes.
pub struct MpeTracker {
    lower: Option<MpeZone>,
    upper: Option<MpeZone>,
    rpn: [RpnCell; 16],
    expr: [ChannelExpr; 16],
    note_pb_range: u8,
    master_pb_range: u8,
    voices: Vec<MpeVoice>,
}

impl Default for MpeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MpeTracker {
    pub fn new() -> Self {
        Self {
            lower: None,
            upper: None,
            rpn: [RpnCell::default(); 16],
            expr: [ChannelExpr::new(); 16],
            note_pb_range: DEFAULT_NOTE_PB_RANGE,
            master_pb_range: DEFAULT_MASTER_PB_RANGE,
            voices: Vec::new(),
        }
    }

    pub fn lower_zone(&self) -> Option<MpeZone> {
        self.lower
    }

    pub fn upper_zone(&self) -> Option<MpeZone> {
        self.upper
    }

    pub fn note_pitch_bend_range(&self) -> u8 {
        self.note_pb_range
    }

    pub fn master_pitch_bend_range(&self) -> u8 {
        self.master_pb_range
    }

    pub fn voices(&self) -> &[MpeVoice] {
        &self.voices
    }

    pub fn configured(&self) -> bool {
        self.lower.is_some() || self.upper.is_some()
    }

    /// Voices on typical member channels even if no MCM has been seen.
    pub fn likely_mpe(&self) -> bool {
        self.configured() || self.voices.iter().any(|v| v.channel > 0 && v.channel < 15)
    }

    pub fn mode_summary(&self) -> String {
        if self.configured() {
            let mut bits = Vec::new();
            if let Some(z) = self.lower {
                bits.push(format!("lower +{}", z.members));
            }
            if let Some(z) = self.upper {
                bits.push(format!("upper +{}", z.members));
            }
            format!(
                "MPE on ({})  note PB ±{}  master PB ±{}",
                bits.join(", "),
                self.note_pb_range,
                self.master_pb_range
            )
        } else if self.likely_mpe() {
            "No MCM — notes on member channels. Keyboard may already be in MPE; send Lower zone to confirm."
                .into()
        } else {
            format!(
                "Not in MPE (no MCM). Default PB ±{} / master ±{}",
                self.note_pb_range, self.master_pb_range
            )
        }
    }

    pub fn role(&self, channel: u8) -> &'static str {
        if self.lower.is_some_and(|z| z.is_master(channel)) {
            "Lower master"
        } else if self.upper.is_some_and(|z| z.is_master(channel)) {
            "Upper master"
        } else if self.lower.is_some_and(|z| z.contains_member(channel)) {
            "Lower member"
        } else if self.upper.is_some_and(|z| z.contains_member(channel)) {
            "Upper member"
        } else {
            "—"
        }
    }

    pub fn push(&mut self, packet: &UmpMessage) {
        if packet.message_type() == 0x4 {
            for p in crate::midi2::downscale_to_midi1(packet) {
                if p.message_type() == 0x2 {
                    self.push_midi1(&p);
                }
            }
            return;
        }
        if packet.message_type() != 0x2 {
            return;
        }
        self.push_midi1(packet);
    }

    fn push_midi1(&mut self, packet: &UmpMessage) {
        let status = packet.status_byte();
        let ch = status & 0x0F;
        let d1 = packet.data1();
        let d2 = packet.data2();
        match status & 0xF0 {
            0x80 => self.note_off(ch, d1),
            0x90 => {
                if d2 == 0 {
                    self.note_off(ch, d1);
                } else {
                    self.note_on(ch, d1, d2);
                }
            }
            0xB0 => self.control(ch, d1, d2),
            0xD0 => {
                self.expr[usize::from(ch)].pressure = d1;
                for v in &mut self.voices {
                    if v.channel == ch {
                        v.pressure = d1;
                    }
                }
            }
            0xE0 => {
                let pb = u16::from(d1) | (u16::from(d2) << 7);
                self.expr[usize::from(ch)].pitch_bend = pb;
                for v in &mut self.voices {
                    if v.channel == ch {
                        v.pitch_bend = pb;
                    }
                }
            }
            _ => {}
        }
    }

    fn note_on(&mut self, ch: u8, note: u8, vel: u8) {
        self.voices.retain(|v| !(v.channel == ch && v.note == note));
        if self.voices.len() >= 128 {
            self.voices.remove(0);
        }
        let e = self.expr[usize::from(ch)];
        self.voices.push(MpeVoice {
            channel: ch,
            note,
            velocity: vel,
            pitch_bend: e.pitch_bend,
            pressure: e.pressure,
            timbre: e.timbre,
        });
    }

    fn note_off(&mut self, ch: u8, note: u8) {
        self.voices.retain(|v| !(v.channel == ch && v.note == note));
    }

    fn control(&mut self, ch: u8, cc: u8, value: u8) {
        match cc {
            CC_TIMBRE => {
                self.expr[usize::from(ch)].timbre = value;
                for v in &mut self.voices {
                    if v.channel == ch {
                        v.timbre = value;
                    }
                }
            }
            CC_ALL_SOUND_OFF | CC_ALL_NOTES_OFF => {
                self.voices.retain(|v| v.channel != ch);
            }
            CC_RESET => {
                self.expr[usize::from(ch)] = ChannelExpr::new();
            }
            CC_RPN_MSB => {
                self.rpn[usize::from(ch)].msb = Some(value);
                self.rpn[usize::from(ch)].is_rpn = true;
            }
            CC_RPN_LSB => {
                self.rpn[usize::from(ch)].lsb = Some(value);
                self.rpn[usize::from(ch)].is_rpn = true;
            }
            CC_NRPN_MSB | CC_NRPN_LSB => {
                self.rpn[usize::from(ch)].is_rpn = false;
            }
            CC_DATA_MSB => self.apply_data_entry(ch, value),
            _ => {}
        }
    }

    fn apply_data_entry(&mut self, ch: u8, msb: u8) {
        let cell = self.rpn[usize::from(ch)];
        if !cell.is_rpn {
            return;
        }
        let Some(param) = cell.msb.zip(cell.lsb) else {
            return;
        };
        if param == MPE_CONFIG_RPN {
            self.apply_mcm(ch, msb.min(15));
        } else if param == PITCH_BEND_RANGE_RPN {
            if self.lower.is_some_and(|z| z.is_master(ch))
                || self.upper.is_some_and(|z| z.is_master(ch))
            {
                self.master_pb_range = msb;
            } else {
                self.note_pb_range = msb;
            }
        }
    }

    fn apply_mcm(&mut self, master: u8, members: u8) {
        match master {
            0 => {
                self.lower = if members == 0 {
                    None
                } else {
                    Some(MpeZone {
                        kind: MpeZoneKind::Lower,
                        master: 0,
                        members,
                    })
                };
            }
            15 => {
                self.upper = if members == 0 {
                    None
                } else {
                    Some(MpeZone {
                        kind: MpeZoneKind::Upper,
                        master: 15,
                        members,
                    })
                };
            }
            _ => {}
        }
    }

    pub fn clear_voices(&mut self) {
        self.voices.clear();
    }
}

/// MIDI 1.0 packets that set an MPE zone (RPN 6) on the master channel.
pub fn mcm_packets(zone: MpeZoneKind, members: u8) -> Vec<UmpMessage> {
    let ch = match zone {
        MpeZoneKind::Lower => 0,
        MpeZoneKind::Upper => 15,
    };
    let n = members.min(15);
    let cc = |controller, value| UmpMessage::midi1_channel_voice(0, 0xB0 | ch, controller, value);
    vec![
        cc(CC_RPN_MSB, 0x00),
        cc(CC_RPN_LSB, 0x06),
        cc(CC_DATA_MSB, n),
        cc(CC_RPN_MSB, 0x7F),
        cc(CC_RPN_LSB, 0x7F),
    ]
}

/// RPN 0 (pitch bend sensitivity) on `channel`. Data Entry MSB = semitones.
pub fn pitch_bend_range_packets(channel: u8, semitones: u8) -> Vec<UmpMessage> {
    let ch = channel & 0x0F;
    let n = semitones.min(96);
    let cc = |controller, value| UmpMessage::midi1_channel_voice(0, 0xB0 | ch, controller, value);
    vec![
        cc(CC_RPN_MSB, 0x00),
        cc(CC_RPN_LSB, 0x00),
        cc(CC_DATA_MSB, n),
        cc(CC_RPN_MSB, 0x7F),
        cc(CC_RPN_LSB, 0x7F),
    ]
}

/// Convert 14-bit pitch bend to approximate semitones using the given range.
pub fn bend_semitones(pitch_bend: u16, range: u8) -> f32 {
    let centered = i32::from(pitch_bend) - 8192;
    (centered as f32 / 8192.0) * f32::from(range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(ch: u8, controller: u8, value: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0xB0 | ch, controller, value)
    }

    fn note_on(ch: u8, note: u8, vel: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x90 | ch, note, vel)
    }

    fn note_off(ch: u8, note: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x80 | ch, note, 0)
    }

    fn pb(ch: u8, value: u16) -> UmpMessage {
        let lsb = (value & 0x7F) as u8;
        let msb = ((value >> 7) & 0x7F) as u8;
        UmpMessage::midi1_channel_voice(0, 0xE0 | ch, lsb, msb)
    }

    #[test]
    fn mcm_sets_lower_zone_members() {
        let mut t = MpeTracker::new();
        for p in mcm_packets(MpeZoneKind::Lower, 7) {
            t.push(&p);
        }
        let z = t.lower_zone().unwrap();
        assert_eq!(z.master, 0);
        assert_eq!(z.members, 7);
        assert!(z.contains_member(1));
        assert!(z.contains_member(7));
        assert!(!z.contains_member(8));
        assert_eq!(t.role(0), "Lower master");
        assert_eq!(t.role(3), "Lower member");
    }

    #[test]
    fn mcm_zero_disables_zone() {
        let mut t = MpeTracker::new();
        for p in mcm_packets(MpeZoneKind::Lower, 5) {
            t.push(&p);
        }
        for p in mcm_packets(MpeZoneKind::Lower, 0) {
            t.push(&p);
        }
        assert!(t.lower_zone().is_none());
    }

    #[test]
    fn member_note_and_expression() {
        let mut t = MpeTracker::new();
        for p in mcm_packets(MpeZoneKind::Lower, 15) {
            t.push(&p);
        }
        t.push(&note_on(2, 60, 100));
        t.push(&pb(2, 8192 + 4096));
        t.push(&cc(2, 74, 90));
        t.push(&UmpMessage::midi1_channel_voice(0, 0xD2, 40, 0));
        assert_eq!(t.voices().len(), 1);
        let v = t.voices()[0];
        assert_eq!(v.channel, 2);
        assert_eq!(v.note, 60);
        assert_eq!(v.pitch_bend, 8192 + 4096);
        assert_eq!(v.timbre, 90);
        assert_eq!(v.pressure, 40);
        t.push(&note_off(2, 60));
        assert!(t.voices().is_empty());
    }

    #[test]
    fn midi2_note_on_tracks_voice() {
        let mut t = MpeTracker::new();
        let m2 = UmpMessage::midi2_channel_voice(0, 0x92, 60, 0, 0x8000_0000);
        t.push(&m2);
        assert_eq!(t.voices().len(), 1);
        assert_eq!(t.voices()[0].channel, 2);
        assert_eq!(t.voices()[0].note, 60);
    }

    #[test]
    fn rpn0_on_master_sets_master_bend_range() {
        let mut t = MpeTracker::new();
        for p in mcm_packets(MpeZoneKind::Lower, 4) {
            t.push(&p);
        }
        t.push(&cc(0, 101, 0));
        t.push(&cc(0, 100, 0));
        t.push(&cc(0, 6, 2));
        assert_eq!(t.master_pitch_bend_range(), 2);
        t.push(&cc(2, 101, 0));
        t.push(&cc(2, 100, 0));
        t.push(&cc(2, 6, 48));
        assert_eq!(t.note_pitch_bend_range(), 48);
    }

    #[test]
    fn voices_capped_at_128() {
        let mut t = MpeTracker::new();
        for i in 0..200u16 {
            t.push(&note_on((i % 16) as u8, (i / 16) as u8, 100));
        }
        assert_eq!(t.voices().len(), 128);
    }

    #[test]
    fn mcm_packet_shape() {
        let pk = mcm_packets(MpeZoneKind::Upper, 3);
        assert_eq!(pk[0].status_byte(), 0xBF);
        assert_eq!(pk[1].data1(), 100);
        assert_eq!(pk[1].data2(), 6);
        assert_eq!(pk[2].data1(), 6);
        assert_eq!(pk[2].data2(), 3);
    }

    #[test]
    fn mode_summary_and_pb_packets() {
        let mut t = MpeTracker::new();
        assert!(t.mode_summary().contains("Not in MPE"));
        t.push(&note_on(3, 60, 100));
        assert!(t.likely_mpe());
        assert!(t.mode_summary().contains("No MCM"));
        for p in mcm_packets(MpeZoneKind::Lower, 15) {
            t.push(&p);
        }
        assert!(t.mode_summary().contains("MPE on"));
        let pb = pitch_bend_range_packets(0, 2);
        assert_eq!(pb[1].data2(), 0);
        assert_eq!(pb[2].data2(), 2);
    }
}
