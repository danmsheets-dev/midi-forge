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
            } => format!("Ch{} CC{controller} {value}", channel + 1),
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
            Self::Other {
                message_type,
                status,
                ..
            } => format!("UMP type {message_type:#X} status {status:#04X}"),
        }
    }
}

pub fn decode(msg: &UmpMessage) -> Decoded {
    let group = msg.group();
    match msg.message_type() {
        0x1 => decode_system(group, msg.status_byte()),
        0x2 => decode_midi1_channel(group, msg.words()[0]),
        0x3 => decode_sysex7(group, msg),
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
    }

    #[test]
    fn decodes_clock() {
        let msg = UmpMessage::midi1_system(0, 0xF8, 0, 0);
        assert_eq!(decode(&msg), Decoded::Clock { group: 0 });
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
}
