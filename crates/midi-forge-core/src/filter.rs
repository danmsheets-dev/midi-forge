use serde::{Deserialize, Serialize};

use crate::ump::UmpMessage;

/// Which family a packet belongs to for thru filtering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    Note,
    PolyPressure,
    ControlChange,
    ProgramChange,
    ChannelPressure,
    PitchBend,
    Sysex,
    Sysex8,
    Clock,
    Transport,
    ActiveSensing,
    Reset,
    SystemCommon,
    PerNote,
    Utility,
    Other,
}

pub fn message_kind(msg: &UmpMessage) -> MessageKind {
    match msg.message_type() {
        0x2 => match msg.status_byte() & 0xF0 {
            0x80 | 0x90 => MessageKind::Note,
            0xA0 => MessageKind::PolyPressure,
            0xB0 => MessageKind::ControlChange,
            0xC0 => MessageKind::ProgramChange,
            0xD0 => MessageKind::ChannelPressure,
            0xE0 => MessageKind::PitchBend,
            _ => MessageKind::Other,
        },
        0x4 => match msg.status_byte() & 0xF0 {
            0x80 | 0x90 => MessageKind::Note,
            0xA0 => MessageKind::PolyPressure,
            0xB0 => MessageKind::ControlChange,
            0xC0 => MessageKind::ProgramChange,
            0xD0 => MessageKind::ChannelPressure,
            0xE0 => MessageKind::PitchBend,
            0x20 | 0x30 | 0x40 | 0x50 => MessageKind::ControlChange,
            0x00 | 0x10 | 0xF0 => MessageKind::PerNote,
            0x60 => MessageKind::PitchBend,
            _ => MessageKind::Other,
        },
        0x0 => MessageKind::Utility,
        0x3 => MessageKind::Sysex,
        0x5 => MessageKind::Sysex8,
        0x1 => match msg.status_byte() {
            0xF8 => MessageKind::Clock,
            0xFA..=0xFC => MessageKind::Transport,
            0xFE => MessageKind::ActiveSensing,
            0xFF => MessageKind::Reset,
            0xF1..=0xF6 => MessageKind::SystemCommon,
            _ => MessageKind::Other,
        },
        _ => MessageKind::Other,
    }
}

/// Per-connection thru filter. Default is pass-through.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub notes: bool,
    pub poly_pressure: bool,
    pub control_change: bool,
    pub program_change: bool,
    pub channel_pressure: bool,
    pub pitch_bend: bool,
    pub sysex: bool,
    /// UMP type 0x5 SysEx8 and MixData. Missing field defaults true.
    #[serde(default = "default_true")]
    pub sysex8: bool,
    pub clock: bool,
    pub transport: bool,
    pub active_sensing: bool,
    pub reset: bool,
    pub system_common: bool,
    pub other: bool,
    /// MIDI 2 per-note controllers, per-note bend is pitch_bend.
    #[serde(default = "default_true")]
    pub per_note: bool,
    /// UMP Utility (type 0x0): NOOP, JR clock/timestamp, DCTPQ, delta clockstamp.
    #[serde(default = "default_true")]
    pub utility: bool,
    /// Bit `i` enables MIDI channel `i` (0–15).
    pub channels: u16,
    /// If set, rewrite channel-voice packets to this channel after the mask.
    pub force_channel: Option<u8>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            notes: true,
            poly_pressure: true,
            control_change: true,
            program_change: true,
            channel_pressure: true,
            pitch_bend: true,
            sysex: true,
            sysex8: true,
            clock: true,
            transport: true,
            active_sensing: true,
            reset: true,
            system_common: true,
            other: true,
            per_note: true,
            utility: true,
            channels: 0xFFFF,
            force_channel: None,
        }
    }
}

fn default_true() -> bool {
    true
}

impl Filter {
    pub fn channel_enabled(&self, channel: u8) -> bool {
        let bit = channel.min(15);
        self.channels & (1 << bit) != 0
    }

    pub fn set_channel_enabled(&mut self, channel: u8, enabled: bool) {
        let bit = channel.min(15);
        if enabled {
            self.channels |= 1 << bit;
        } else {
            self.channels &= !(1 << bit);
        }
    }

    pub fn set_all_channels(&mut self, enabled: bool) {
        self.channels = if enabled { 0xFFFF } else { 0 };
    }

    fn kind_enabled(&self, kind: MessageKind) -> bool {
        match kind {
            MessageKind::Note => self.notes,
            MessageKind::PolyPressure => self.poly_pressure,
            MessageKind::ControlChange => self.control_change,
            MessageKind::ProgramChange => self.program_change,
            MessageKind::ChannelPressure => self.channel_pressure,
            MessageKind::PitchBend => self.pitch_bend,
            MessageKind::Sysex => self.sysex,
            MessageKind::Sysex8 => self.sysex8,
            MessageKind::Clock => self.clock,
            MessageKind::Transport => self.transport,
            MessageKind::ActiveSensing => self.active_sensing,
            MessageKind::Reset => self.reset,
            MessageKind::SystemCommon => self.system_common,
            MessageKind::PerNote => self.per_note,
            MessageKind::Utility => self.utility,
            MessageKind::Other => self.other,
        }
    }

    /// Drop or rewrite a packet. `None` means do not send.
    pub fn apply(&self, packet: &UmpMessage) -> Option<UmpMessage> {
        if !self.kind_enabled(message_kind(packet)) {
            return None;
        }
        if let Some(ch) = packet.channel()
            && !self.channel_enabled(ch)
        {
            return None;
        }
        match self.force_channel {
            Some(ch) if packet.channel().is_some() => Some(packet.with_channel(ch)),
            _ => Some(*packet),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ump::UmpMessage;

    fn note_on(channel: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x90 | channel, 60, 127)
    }

    fn clock() -> UmpMessage {
        UmpMessage::midi1_system(0, 0xF8, 0, 0)
    }

    #[test]
    fn default_passes_notes_and_clock() {
        let f = Filter::default();
        assert_eq!(f.apply(&note_on(0)), Some(note_on(0)));
        assert_eq!(f.apply(&clock()), Some(clock()));
    }

    #[test]
    fn strip_clock_keeps_notes() {
        let f = Filter {
            clock: false,
            ..Filter::default()
        };
        assert_eq!(f.apply(&clock()), None);
        assert_eq!(f.apply(&note_on(0)), Some(note_on(0)));
    }

    #[test]
    fn channel_mask_drops_other_channels() {
        let mut f = Filter::default();
        f.set_all_channels(false);
        f.set_channel_enabled(0, true);
        assert_eq!(f.apply(&note_on(0)), Some(note_on(0)));
        assert_eq!(f.apply(&note_on(1)), None);
        assert_eq!(f.apply(&clock()), Some(clock()));
    }

    #[test]
    fn force_channel_remaps_after_mask() {
        let mut f = Filter::default();
        f.set_all_channels(false);
        f.set_channel_enabled(0, true);
        f.force_channel = Some(3);
        assert_eq!(f.apply(&note_on(0)), Some(note_on(3)));
        assert_eq!(f.apply(&note_on(2)), None);
        assert_eq!(f.apply(&clock()), Some(clock()));
    }

    #[test]
    fn dropping_notes_still_passes_cc() {
        let f = Filter {
            notes: false,
            ..Filter::default()
        };
        let cc = UmpMessage::midi1_channel_voice(0, 0xB0, 7, 64);
        assert_eq!(f.apply(&note_on(0)), None);
        assert_eq!(f.apply(&cc), Some(cc));
    }

    #[test]
    fn midi2_notes_obey_note_filter_and_channel() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0x90, 60, 0, 0x8000_0000);
        let drop_notes = Filter {
            notes: false,
            ..Filter::default()
        };
        assert_eq!(drop_notes.apply(&m2), None);

        let mut ch0 = Filter::default();
        ch0.set_all_channels(false);
        ch0.set_channel_enabled(0, true);
        ch0.force_channel = Some(4);
        let out = ch0.apply(&m2).expect("channel 0 MIDI 2 note");
        assert_eq!(out.channel(), Some(4));
        assert_eq!(out.message_type(), 0x4);
        assert_eq!(out.data1(), 60);
    }

    #[test]
    fn midi2_rpn_is_control_change() {
        let rpn = UmpMessage::midi2_channel_voice(0, 0x20, 0, 6, 0);
        assert_eq!(message_kind(&rpn), MessageKind::ControlChange);
        let f = Filter {
            control_change: false,
            ..Filter::default()
        };
        assert_eq!(f.apply(&rpn), None);
    }

    #[test]
    fn midi2_per_note_has_own_filter() {
        let pn = UmpMessage::midi2_channel_voice(0, 0x00, 60, 7, 1);
        assert_eq!(message_kind(&pn), MessageKind::PerNote);
        let f = Filter {
            per_note: false,
            ..Filter::default()
        };
        assert_eq!(f.apply(&pn), None);
        assert!(Filter::default().apply(&pn).is_some());
    }

    #[test]
    fn default_passes_utility_jr() {
        let jr = UmpMessage::from_word(0x0020_0100).unwrap();
        assert_eq!(message_kind(&jr), MessageKind::Utility);
        assert_eq!(Filter::default().apply(&jr), Some(jr));
        let drop_other = Filter {
            other: false,
            ..Filter::default()
        };
        assert_eq!(drop_other.apply(&jr), Some(jr));
        let drop_utility = Filter {
            utility: false,
            ..Filter::default()
        };
        assert_eq!(drop_utility.apply(&jr), None);
    }

    #[test]
    fn missing_utility_field_defaults_true() {
        let json = r#"{"notes":true,"poly_pressure":true,"control_change":true,"program_change":true,"channel_pressure":true,"pitch_bend":true,"sysex":true,"clock":true,"transport":true,"active_sensing":true,"reset":true,"system_common":true,"other":true,"per_note":true,"channels":65535,"force_channel":null}"#;
        let f: Filter = serde_json::from_str(json).unwrap();
        assert!(f.utility);
        assert!(f.sysex8);
    }

    #[test]
    fn type5_sysex8_passes_when_other_is_dropped() {
        let m = UmpMessage::try_from_words(&[0x5002_AB01, 0, 0, 0]).unwrap();
        assert_eq!(message_kind(&m), MessageKind::Sysex8);
        let drop_other = Filter {
            other: false,
            ..Filter::default()
        };
        assert_eq!(drop_other.apply(&m), Some(m));
        let drop_sysex8 = Filter {
            sysex8: false,
            ..Filter::default()
        };
        assert_eq!(drop_sysex8.apply(&m), None);
        assert!(Filter::default().apply(&m).is_some());
    }

    #[test]
    fn mixdata_shares_sysex8_kind() {
        let m = UmpMessage::try_from_words(&[0x5285_0000, 0, 0, 0]).unwrap();
        assert_eq!(message_kind(&m), MessageKind::Sysex8);
        let drop_sysex8 = Filter {
            sysex8: false,
            ..Filter::default()
        };
        assert_eq!(drop_sysex8.apply(&m), None);
    }
}
