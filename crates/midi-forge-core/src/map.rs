use serde::{Deserialize, Serialize};

use crate::ump::UmpMessage;

/// Channel-voice types a data map can match or emit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceKind {
    NoteOff,
    NoteOn,
    PolyPressure,
    ControlChange,
    ProgramChange,
    ChannelPressure,
    PitchBend,
}

impl VoiceKind {
    pub fn from_packet(msg: &UmpMessage) -> Option<Self> {
        if !matches!(msg.message_type(), 0x2 | 0x4) {
            return None;
        }
        Some(match msg.status_byte() & 0xF0 {
            0x80 => Self::NoteOff,
            0x90 => Self::NoteOn,
            0xA0 => Self::PolyPressure,
            0xB0 | 0x00 | 0x10 | 0x20 | 0x30 | 0x40 | 0x50 => Self::ControlChange,
            0xC0 => Self::ProgramChange,
            0xD0 => Self::ChannelPressure,
            0xE0 | 0x60 => Self::PitchBend,
            _ => return None,
        })
    }

    pub fn status_nibble(self) -> u8 {
        match self {
            Self::NoteOff => 0x80,
            Self::NoteOn => 0x90,
            Self::PolyPressure => 0xA0,
            Self::ControlChange => 0xB0,
            Self::ProgramChange => 0xC0,
            Self::ChannelPressure => 0xD0,
            Self::PitchBend => 0xE0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NoteOff => "Note Off",
            Self::NoteOn => "Note On",
            Self::PolyPressure => "Poly press",
            Self::ControlChange => "CC",
            Self::ProgramChange => "Program",
            Self::ChannelPressure => "Chan press",
            Self::PitchBend => "Bend",
        }
    }

    pub fn all() -> [Self; 7] {
        [
            Self::NoteOff,
            Self::NoteOn,
            Self::PolyPressure,
            Self::ControlChange,
            Self::ProgramChange,
            Self::ChannelPressure,
            Self::PitchBend,
        ]
    }
}

/// Which incoming channel-voice messages an entry matches.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    AnyChannelVoice,
    Notes,
    One(VoiceKind),
}

impl MatchKind {
    fn matches(self, kind: VoiceKind) -> bool {
        match self {
            Self::AnyChannelVoice => true,
            Self::Notes => matches!(kind, VoiceKind::NoteOn | VoiceKind::NoteOff),
            Self::One(want) => kind == want,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AnyChannelVoice => "Any voice",
            Self::Notes => "Notes",
            Self::One(kind) => kind.label(),
        }
    }
}

/// 7-bit value transform.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueMap {
    #[default]
    Keep,
    Constant(u8),
    Offset(i16),
    Scale {
        in_min: u8,
        in_max: u8,
        out_min: u8,
        out_max: u8,
        invert: bool,
    },
}

impl ValueMap {
    pub fn apply(&self, value: u8) -> u8 {
        match *self {
            Self::Keep => value.min(127),
            Self::Constant(v) => v.min(127),
            Self::Offset(delta) => (i16::from(value) + delta).clamp(0, 127) as u8,
            Self::Scale {
                in_min,
                in_max,
                out_min,
                out_max,
                invert,
            } => {
                let v = if invert {
                    127u8.saturating_sub(value)
                } else {
                    value
                };
                scale_7bit(v, in_min, in_max, out_min, out_max)
            }
        }
    }
}

fn scale_7bit(value: u8, in_min: u8, in_max: u8, out_min: u8, out_max: u8) -> u8 {
    let v = value.clamp(in_min.min(in_max), in_min.max(in_max));
    if in_max == in_min {
        return out_min.min(127);
    }
    let t = i32::from(v) - i32::from(in_min);
    let den = i32::from(in_max) - i32::from(in_min);
    let mapped = i32::from(out_min) + t * (i32::from(out_max) - i32::from(out_min)) / den;
    mapped.clamp(0, 127) as u8
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Matcher {
    pub kind: MatchKind,
    pub channels: u16,
    pub data1_min: u8,
    pub data1_max: u8,
    pub data2_min: u8,
    pub data2_max: u8,
}

impl Default for Matcher {
    fn default() -> Self {
        Self {
            kind: MatchKind::AnyChannelVoice,
            channels: 0xFFFF,
            data1_min: 0,
            data1_max: 127,
            data2_min: 0,
            data2_max: 127,
        }
    }
}

impl Matcher {
    /// Capture a CC or note into a tight matcher (this number, this channel).
    pub fn learn_from(packet: &UmpMessage) -> Option<Self> {
        let packet = if packet.message_type() == 0x4 {
            crate::midi2::downscale_to_midi1(packet)
                .into_iter()
                .find(|p| p.message_type() == 0x2)?
        } else {
            *packet
        };
        let kind = VoiceKind::from_packet(&packet)?;
        if matches!(kind, VoiceKind::NoteOn) && packet.data2() == 0 {
            return None;
        }
        let ch = packet.channel()?;
        let d1 = packet.data1();
        Some(Self {
            kind: MatchKind::One(kind),
            channels: 1 << ch.min(15),
            data1_min: d1,
            data1_max: d1,
            data2_min: 0,
            data2_max: 127,
        })
    }

    pub fn matches(&self, packet: &UmpMessage) -> bool {
        let Some(kind) = VoiceKind::from_packet(packet) else {
            return false;
        };
        if !self.kind.matches(kind) {
            return false;
        }
        if let Some(ch) = packet.channel()
            && self.channels & (1 << ch.min(15)) == 0
        {
            return false;
        }
        let (d1, d2) = if packet.message_type() == 0x4 {
            let m1 = crate::midi2::downscale_to_midi1(packet);
            let p = m1.last().copied().unwrap_or(*packet);
            (p.data1(), p.data2())
        } else {
            (packet.data1(), packet.data2())
        };
        in_range(d1, self.data1_min, self.data1_max) && in_range(d2, self.data2_min, self.data2_max)
    }
}

fn in_range(v: u8, min: u8, max: u8) -> bool {
    let lo = min.min(max);
    let hi = min.max(max);
    v >= lo && v <= hi
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapAction {
    Drop,
    Rewrite {
        #[serde(default)]
        kind: Option<VoiceKind>,
        #[serde(default)]
        channel: Option<u8>,
        #[serde(default)]
        data1: ValueMap,
        #[serde(default)]
        data2: ValueMap,
    },
}

fn vel7_to_16(v: u8) -> u16 {
    if v == 0 {
        0
    } else {
        (u32::from(v) * 65535 / 127) as u16
    }
}

fn rewrite_midi2(
    packet: &UmpMessage,
    out_kind: VoiceKind,
    status: u8,
    data1: &ValueMap,
    data2: &ValueMap,
) -> UmpMessage {
    let d1 = data1.apply(packet.data1());
    let old_w1 = packet.words().get(1).copied().unwrap_or(0);
    let w1 = if matches!(data2, ValueMap::Keep) {
        old_w1
    } else {
        let scaled = crate::midi2::downscale_to_midi1(packet);
        let src_d2 = scaled.last().map(|p| p.data2()).unwrap_or(0);
        let d2 = data2.apply(src_d2);
        match out_kind {
            VoiceKind::NoteOn | VoiceKind::NoteOff => {
                (u32::from(vel7_to_16(d2)) << 16) | (old_w1 & 0xFFFF)
            }
            VoiceKind::ControlChange | VoiceKind::PolyPressure | VoiceKind::ChannelPressure => {
                u32::from(d2) << 25
            }
            VoiceKind::PitchBend => u32::from(d2) << 25,
            VoiceKind::ProgramChange => u32::from(d2) << 24,
        }
    };
    UmpMessage::midi2_channel_voice(packet.group(), status, d1, packet.data2(), w1)
}

impl MapAction {
    fn apply(&self, packet: &UmpMessage) -> Option<UmpMessage> {
        match self {
            Self::Drop => None,
            Self::Rewrite {
                kind,
                channel,
                data1,
                data2,
            } => {
                let src_kind = VoiceKind::from_packet(packet)?;
                let out_kind = kind.unwrap_or(src_kind);
                let ch = channel.unwrap_or_else(|| packet.channel().unwrap_or(0)) & 0x0F;
                let status = out_kind.status_nibble() | ch;
                if packet.message_type() == 0x4 {
                    return Some(rewrite_midi2(packet, out_kind, status, data1, data2));
                }
                let d1 = data1.apply(packet.data1());
                let d2 = data2.apply(packet.data2());
                Some(UmpMessage::midi1_channel_voice(
                    packet.group(),
                    status,
                    d1,
                    d2,
                ))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapEntry {
    pub matcher: Matcher,
    pub action: MapAction,
}

/// Ordered data map. First matching entry wins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DataMap {
    pub entries: Vec<MapEntry>,
    pub pass_unmatched: bool,
}

impl Default for DataMap {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            pass_unmatched: true,
        }
    }
}

impl DataMap {
    /// Insert a learned matcher at the front (first match wins). Identity rewrite.
    pub fn learn_insert(&mut self, packet: &UmpMessage) -> Option<String> {
        let matcher = Matcher::learn_from(packet)?;
        let label = match matcher.kind {
            MatchKind::One(VoiceKind::ControlChange) => {
                format!(
                    "CC{} Ch{}",
                    matcher.data1_min,
                    matcher.channels.trailing_zeros() + 1
                )
            }
            MatchKind::One(VoiceKind::NoteOn) => {
                format!(
                    "Note {} Ch{}",
                    matcher.data1_min,
                    matcher.channels.trailing_zeros() + 1
                )
            }
            MatchKind::One(k) => {
                format!("{} Ch{}", k.label(), matcher.channels.trailing_zeros() + 1)
            }
            _ => "voice".into(),
        };
        self.entries.insert(
            0,
            MapEntry {
                matcher,
                action: MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Keep,
                    data2: ValueMap::Keep,
                },
            },
        );
        Some(label)
    }

    pub fn apply(&self, packet: &UmpMessage) -> Option<UmpMessage> {
        if VoiceKind::from_packet(packet).is_none() {
            return Some(*packet);
        }
        for entry in &self.entries {
            if entry.matcher.matches(packet) {
                return entry.action.apply(packet);
            }
        }
        if self.pass_unmatched {
            Some(*packet)
        } else {
            None
        }
    }

    pub fn transpose(semitones: i16) -> Self {
        Self {
            pass_unmatched: true,
            entries: vec![MapEntry {
                matcher: Matcher {
                    kind: MatchKind::Notes,
                    ..Matcher::default()
                },
                action: MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Offset(semitones),
                    data2: ValueMap::Keep,
                },
            }],
        }
    }

    pub fn remap_cc(from: u8, to: u8) -> Self {
        Self {
            pass_unmatched: true,
            entries: vec![MapEntry {
                matcher: Matcher {
                    kind: MatchKind::One(VoiceKind::ControlChange),
                    data1_min: from,
                    data1_max: from,
                    ..Matcher::default()
                },
                action: MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Constant(to),
                    data2: ValueMap::Keep,
                },
            }],
        }
    }

    pub fn invert_velocity() -> Self {
        Self {
            pass_unmatched: true,
            entries: vec![MapEntry {
                matcher: Matcher {
                    kind: MatchKind::Notes,
                    ..Matcher::default()
                },
                action: MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Keep,
                    data2: ValueMap::Scale {
                        in_min: 0,
                        in_max: 127,
                        out_min: 0,
                        out_max: 127,
                        invert: true,
                    },
                },
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note_on(note: u8, vel: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x90, note, vel)
    }

    fn cc(controller: u8, value: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0xB0, controller, value)
    }

    fn clock() -> UmpMessage {
        UmpMessage::midi1_system(0, 0xF8, 0, 0)
    }

    #[test]
    fn empty_map_is_identity() {
        let map = DataMap::default();
        assert_eq!(map.apply(&note_on(60, 100)), Some(note_on(60, 100)));
        assert_eq!(map.apply(&clock()), Some(clock()));
    }

    #[test]
    fn transpose_notes_clamps() {
        let map = DataMap::transpose(12);
        assert_eq!(map.apply(&note_on(60, 100)), Some(note_on(72, 100)));
        let low = DataMap::transpose(-70);
        assert_eq!(low.apply(&note_on(60, 1)), Some(note_on(0, 1)));
        assert_eq!(map.apply(&cc(1, 64)), Some(cc(1, 64)));
    }

    #[test]
    fn remap_cc_number() {
        let map = DataMap::remap_cc(1, 7);
        assert_eq!(map.apply(&cc(1, 64)), Some(cc(7, 64)));
        assert_eq!(map.apply(&cc(2, 64)), Some(cc(2, 64)));
    }

    #[test]
    fn drop_matching_cc() {
        let map = DataMap {
            pass_unmatched: true,
            entries: vec![MapEntry {
                matcher: Matcher {
                    kind: MatchKind::One(VoiceKind::ControlChange),
                    ..Matcher::default()
                },
                action: MapAction::Drop,
            }],
        };
        assert_eq!(map.apply(&cc(1, 10)), None);
        assert_eq!(map.apply(&note_on(60, 1)), Some(note_on(60, 1)));
    }

    #[test]
    fn convert_cc_to_note_on() {
        let map = DataMap {
            pass_unmatched: true,
            entries: vec![MapEntry {
                matcher: Matcher {
                    kind: MatchKind::One(VoiceKind::ControlChange),
                    data1_min: 20,
                    data1_max: 20,
                    ..Matcher::default()
                },
                action: MapAction::Rewrite {
                    kind: Some(VoiceKind::NoteOn),
                    channel: None,
                    data1: ValueMap::Constant(60),
                    data2: ValueMap::Keep,
                },
            }],
        };
        assert_eq!(map.apply(&cc(20, 90)), Some(note_on(60, 90)));
    }

    #[test]
    fn invert_velocity() {
        let map = DataMap::invert_velocity();
        assert_eq!(map.apply(&note_on(60, 127)), Some(note_on(60, 0)));
        assert_eq!(map.apply(&note_on(60, 0)), Some(note_on(60, 127)));
    }

    #[test]
    fn first_match_wins() {
        let map = DataMap {
            pass_unmatched: true,
            entries: vec![
                MapEntry {
                    matcher: Matcher {
                        kind: MatchKind::Notes,
                        ..Matcher::default()
                    },
                    action: MapAction::Drop,
                },
                MapEntry {
                    matcher: Matcher {
                        kind: MatchKind::Notes,
                        ..Matcher::default()
                    },
                    action: MapAction::Rewrite {
                        kind: None,
                        channel: None,
                        data1: ValueMap::Offset(12),
                        data2: ValueMap::Keep,
                    },
                },
            ],
        };
        assert_eq!(map.apply(&note_on(60, 1)), None);
    }

    #[test]
    fn unmatched_can_drop() {
        let map = DataMap {
            pass_unmatched: false,
            entries: vec![MapEntry {
                matcher: Matcher {
                    kind: MatchKind::One(VoiceKind::ControlChange),
                    ..Matcher::default()
                },
                action: MapAction::Rewrite {
                    kind: None,
                    channel: None,
                    data1: ValueMap::Keep,
                    data2: ValueMap::Keep,
                },
            }],
        };
        assert_eq!(map.apply(&cc(1, 1)), Some(cc(1, 1)));
        assert_eq!(map.apply(&note_on(60, 1)), None);
        assert_eq!(map.apply(&clock()), Some(clock()));
    }

    #[test]
    fn midi2_note_transpose_keeps_ump_type() {
        let map = DataMap::transpose(2);
        let m2 = UmpMessage::midi2_channel_voice(0, 0x90, 60, 0, 0xFFFF_0000);
        let out = map.apply(&m2).expect("mapped");
        assert_eq!(out.message_type(), 0x4);
        assert_eq!(out.data1(), 62);
        assert_eq!(out.words()[1], 0xFFFF_0000);
    }

    #[test]
    fn learn_cc_inserts_front() {
        let mut map = DataMap::default();
        let label = map.learn_insert(&cc(74, 40)).unwrap();
        assert!(label.contains("CC74"));
        assert_eq!(map.entries.len(), 1);
        assert_eq!(map.entries[0].matcher.data1_min, 74);
        assert_eq!(map.entries[0].matcher.data1_max, 74);
        assert!(Matcher::learn_from(&clock()).is_none());
        assert!(Matcher::learn_from(&note_on(60, 0)).is_none());
    }
}
