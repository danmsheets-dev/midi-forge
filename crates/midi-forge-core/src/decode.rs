use crate::ump::UmpMessage;

/// Monitor-facing decode of a UMP packet. Raw words stay on `UmpMessage`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decoded {
    NoteOff {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOn {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        group: u8,
        channel: u8,
        controller: u8,
        value: u8,
    },
    PolyPressure {
        group: u8,
        channel: u8,
        note: u8,
        pressure: u8,
    },
    ProgramChange {
        group: u8,
        channel: u8,
        program: u8,
    },
    ChannelPressure {
        group: u8,
        channel: u8,
        pressure: u8,
    },
    PitchBend {
        group: u8,
        channel: u8,
        lsb: u8,
        msb: u8,
    },
    Clock {
        group: u8,
    },
    Start {
        group: u8,
    },
    Stop {
        group: u8,
    },
    Continue {
        group: u8,
    },
    Sysex7 {
        group: u8,
        status: u8,
        count: u8,
        data: [u8; 6],
    },
    Midi2NoteOn {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    },
    Midi2NoteOff {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
    },
    Midi2ControlChange {
        group: u8,
        channel: u8,
        controller: u8,
        value: u32,
    },
    Midi2PolyPressure {
        group: u8,
        channel: u8,
        note: u8,
        pressure: u32,
    },
    Midi2ProgramChange {
        group: u8,
        channel: u8,
        program: u8,
        bank_msb: u8,
        bank_lsb: u8,
        bank_valid: bool,
    },
    Midi2ChannelPressure {
        group: u8,
        channel: u8,
        pressure: u32,
    },
    Midi2PitchBend {
        group: u8,
        channel: u8,
        value: u32,
    },
    Other {
        message_type: u8,
        group: u8,
        status: u8,
    },
}

impl Decoded {
    pub fn summary(&self) -> String {
        match *self {
            Self::NoteOn {
                channel,
                note,
                velocity,
                ..
            } => format!("Ch{} NoteOn {note} vel {velocity}", channel + 1),
            Self::NoteOff {
                channel,
                note,
                velocity,
                ..
            } => format!("Ch{} NoteOff {note} vel {velocity}", channel + 1),
            Self::ControlChange {
                channel,
                controller,
                value,
                ..
            } => format!(
                "Ch{} {} {value}",
                channel + 1,
                crate::cc::cc_label(controller)
            ),
            Self::PolyPressure {
                channel,
                note,
                pressure,
                ..
            } => format!("Ch{} PolyPress {note} {pressure}", channel + 1),
            Self::ProgramChange {
                channel, program, ..
            } => format!("Ch{} Program {program}", channel + 1),
            Self::ChannelPressure {
                channel, pressure, ..
            } => format!("Ch{} ChanPress {pressure}", channel + 1),
            Self::PitchBend {
                channel, lsb, msb, ..
            } => format!("Ch{} PitchBend {lsb}/{msb}", channel + 1),
            Self::Clock { .. } => "Clock".to_string(),
            Self::Start { .. } => "Start".to_string(),
            Self::Stop { .. } => "Stop".to_string(),
            Self::Continue { .. } => "Continue".to_string(),
            Self::Sysex7 { count, .. } => format!("SysEx7 {count} bytes"),
            Self::Midi2NoteOn {
                channel,
                note,
                velocity,
                ..
            } => format!("Ch{} M2 NoteOn {note} vel16 {velocity}", channel + 1),
            Self::Midi2NoteOff {
                channel,
                note,
                velocity,
                ..
            } => format!("Ch{} M2 NoteOff {note} vel16 {velocity}", channel + 1),
            Self::Midi2ControlChange {
                channel,
                controller,
                value,
                ..
            } => format!(
                "Ch{} M2 {} {value}",
                channel + 1,
                crate::cc::cc_label(controller)
            ),
            Self::Midi2PolyPressure {
                channel,
                note,
                pressure,
                ..
            } => format!("Ch{} M2 PolyPress {note} {pressure}", channel + 1),
            Self::Midi2ProgramChange {
                channel,
                program,
                bank_valid,
                bank_msb,
                bank_lsb,
                ..
            } => {
                if bank_valid {
                    format!(
                        "Ch{} M2 Program {program} bank {bank_msb}/{bank_lsb}",
                        channel + 1
                    )
                } else {
                    format!("Ch{} M2 Program {program}", channel + 1)
                }
            }
            Self::Midi2ChannelPressure {
                channel, pressure, ..
            } => format!("Ch{} M2 ChanPress {pressure}", channel + 1),
            Self::Midi2PitchBend { channel, value, .. } => {
                format!("Ch{} M2 PitchBend {value}", channel + 1)
            }
            Self::Other {
                message_type,
                status,
                ..
            } => format!("UMP type {message_type:#X} status {status:#04X}"),
        }
    }

    /// Stable Lua/script key for this packet.
    pub fn kind_key(&self) -> &'static str {
        match self {
            Self::NoteOn { .. } => "note_on",
            Self::NoteOff { .. } => "note_off",
            Self::ControlChange { .. } => "cc",
            Self::PolyPressure { .. } => "poly_pressure",
            Self::ProgramChange { .. } => "program",
            Self::ChannelPressure { .. } => "channel_pressure",
            Self::PitchBend { .. } => "pitch_bend",
            Self::Clock { .. } => "clock",
            Self::Start { .. } => "start",
            Self::Stop { .. } => "stop",
            Self::Continue { .. } => "continue",
            Self::Sysex7 { .. } => "sysex",
            Self::Midi2NoteOn { .. } => "m2_note_on",
            Self::Midi2NoteOff { .. } => "m2_note_off",
            Self::Midi2ControlChange { .. } => "m2_cc",
            Self::Midi2PolyPressure { .. } => "m2_poly_pressure",
            Self::Midi2ProgramChange { .. } => "m2_program",
            Self::Midi2ChannelPressure { .. } => "m2_channel_pressure",
            Self::Midi2PitchBend { .. } => "m2_pitch_bend",
            Self::Other { .. } => "other",
        }
    }
}

pub fn decode(msg: &UmpMessage) -> Decoded {
    let group = msg.group();
    match msg.message_type() {
        0x1 => decode_system(group, msg.status_byte()),
        0x2 => decode_midi1_channel(group, msg.words()[0]),
        0x3 => decode_sysex7(group, msg),
        0x4 => decode_midi2_channel(msg),
        mt => Decoded::Other {
            message_type: mt,
            group,
            status: msg.status_byte(),
        },
    }
}

fn decode_system(group: u8, status: u8) -> Decoded {
    match status {
        0xF8 => Decoded::Clock { group },
        0xFA => Decoded::Start { group },
        0xFB => Decoded::Continue { group },
        0xFC => Decoded::Stop { group },
        other => Decoded::Other {
            message_type: 0x1,
            group,
            status: other,
        },
    }
}

fn decode_midi1_channel(group: u8, word: u32) -> Decoded {
    let status = ((word >> 16) & 0xFF) as u8;
    let data1 = ((word >> 8) & 0xFF) as u8;
    let data2 = (word & 0xFF) as u8;
    let channel = status & 0x0F;
    match status & 0xF0 {
        0x80 => Decoded::NoteOff {
            group,
            channel,
            note: data1,
            velocity: data2,
        },
        0x90 => Decoded::NoteOn {
            group,
            channel,
            note: data1,
            velocity: data2,
        },
        0xA0 => Decoded::PolyPressure {
            group,
            channel,
            note: data1,
            pressure: data2,
        },
        0xB0 => Decoded::ControlChange {
            group,
            channel,
            controller: data1,
            value: data2,
        },
        0xC0 => Decoded::ProgramChange {
            group,
            channel,
            program: data1,
        },
        0xD0 => Decoded::ChannelPressure {
            group,
            channel,
            pressure: data1,
        },
        0xE0 => Decoded::PitchBend {
            group,
            channel,
            lsb: data1,
            msb: data2,
        },
        _ => Decoded::Other {
            message_type: 0x2,
            group,
            status,
        },
    }
}

fn decode_midi2_channel(msg: &UmpMessage) -> Decoded {
    let group = msg.group();
    let status = msg.status_byte();
    let channel = status & 0x0F;
    let d1 = msg.data1();
    let w1 = msg.words().get(1).copied().unwrap_or(0);
    match status & 0xF0 {
        0x90 => Decoded::Midi2NoteOn {
            group,
            channel,
            note: d1,
            velocity: (w1 >> 16) as u16,
        },
        0x80 => Decoded::Midi2NoteOff {
            group,
            channel,
            note: d1,
            velocity: (w1 >> 16) as u16,
        },
        0xB0 => Decoded::Midi2ControlChange {
            group,
            channel,
            controller: d1,
            value: w1,
        },
        0xA0 => Decoded::Midi2PolyPressure {
            group,
            channel,
            note: d1,
            pressure: w1,
        },
        0xC0 => Decoded::Midi2ProgramChange {
            group,
            channel,
            program: ((w1 >> 24) & 0x7F) as u8,
            bank_msb: ((w1 >> 8) & 0x7F) as u8,
            bank_lsb: (w1 & 0x7F) as u8,
            bank_valid: d1 & 0x01 != 0,
        },
        0xD0 => Decoded::Midi2ChannelPressure {
            group,
            channel,
            pressure: w1,
        },
        0xE0 => Decoded::Midi2PitchBend {
            group,
            channel,
            value: w1,
        },
        _ => Decoded::Other {
            message_type: 0x4,
            group,
            status,
        },
    }
}

fn decode_sysex7(group: u8, msg: &UmpMessage) -> Decoded {
    let w0 = msg.words()[0];
    let w1 = msg.words().get(1).copied().unwrap_or(0);
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
    Decoded::Sysex7 {
        group,
        status,
        count,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi1::Midi1Parser;
    use crate::ump::UmpMessage;

    #[test]
    fn decodes_note_on() {
        let msg = UmpMessage::midi1_channel_voice(0, 0x90, 60, 127);
        assert_eq!(
            decode(&msg),
            Decoded::NoteOn {
                group: 0,
                channel: 0,
                note: 60,
                velocity: 127
            }
        );
        assert_eq!(decode(&msg).summary(), "Ch1 NoteOn 60 vel 127");
        assert_eq!(decode(&msg).kind_key(), "note_on");
    }

    #[test]
    fn decodes_clock() {
        let msg = UmpMessage::midi1_system(0, 0xF8, 0, 0);
        assert_eq!(decode(&msg), Decoded::Clock { group: 0 });
    }

    #[test]
    fn decodes_named_cc() {
        let msg = UmpMessage::midi1_channel_voice(0, 0xB0, 7, 100);
        assert_eq!(decode(&msg).summary(), "Ch1 CC7 (Volume) 100");
        let unknown = UmpMessage::midi1_channel_voice(0, 0xB1, 20, 1);
        assert_eq!(decode(&unknown).summary(), "Ch2 CC20 1");
    }

    #[test]
    fn decodes_identity_sysex_from_parser() {
        let mut p = Midi1Parser::new();
        let msgs = p.push_slice(&[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]);
        match decode(&msgs[0]) {
            Decoded::Sysex7 {
                count,
                data,
                status,
                ..
            } => {
                assert_eq!(status, 0);
                assert_eq!(count, 4);
                assert_eq!(&data[..4], &[0x7E, 0x7F, 0x06, 0x01]);
            }
            other => panic!("expected sysex, got {other:?}"),
        }
    }

    #[test]
    fn decodes_midi2_note_on() {
        let msg = UmpMessage::midi2_channel_voice(1, 0x92, 64, 0, 0x8000_0000);
        assert_eq!(
            decode(&msg),
            Decoded::Midi2NoteOn {
                group: 1,
                channel: 2,
                note: 64,
                velocity: 0x8000
            }
        );
        assert_eq!(decode(&msg).summary(), "Ch3 M2 NoteOn 64 vel16 32768");
    }

    #[test]
    fn decodes_midi2_cc_and_pitch() {
        let cc = UmpMessage::midi2_channel_voice(0, 0xB0, 7, 0, 0x8000_0000);
        assert_eq!(
            decode(&cc),
            Decoded::Midi2ControlChange {
                group: 0,
                channel: 0,
                controller: 7,
                value: 0x8000_0000
            }
        );
        assert_eq!(decode(&cc).summary(), "Ch1 M2 CC7 (Volume) 2147483648");
        let pb = UmpMessage::midi2_channel_voice(0, 0xE4, 0, 0, 0x8000_0000);
        assert_eq!(decode(&pb).summary(), "Ch5 M2 PitchBend 2147483648");
    }
}
