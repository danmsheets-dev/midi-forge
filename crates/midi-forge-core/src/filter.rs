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
    Clock,
    Transport,
    ActiveSensing,
    Reset,
    SystemCommon,
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
            _ => MessageKind::Other,
        },
        0x3 => MessageKind::Sysex,
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
    pub clock: bool,
    pub transport: bool,
    pub active_sensing: bool,
    pub reset: bool,
    pub system_common: bool,
    pub other: bool,
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
            clock: true,
            transport: true,
            active_sensing: true,
            reset: true,
            system_common: true,
            other: true,
            channels: 0xFFFF,
            force_channel: None,
        }
    }
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
            MessageKind::Clock => self.clock,
            MessageKind::Transport => self.transport,
            MessageKind::ActiveSensing => self.active_sensing,
            MessageKind::Reset => self.reset,
            MessageKind::SystemCommon => self.system_common,
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
}
