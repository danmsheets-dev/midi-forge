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
    SongPosition {
        group: u8,
        beats: u16,
    },
    MtcQuarter {
        group: u8,
        data: u8,
    },
    Sysex7 {
        group: u8,
        status: u8,
        count: u8,
        data: [u8; 6],
    },
    SysEx8 {
        group: u8,
        status: u8,
        stream_id: u8,
        count: u8,
        data: [u8; 13],
    },
    MixData {
        group: u8,
        status: u8,
        mds_id: u8,
    },
    Midi2NoteOn {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute_data: u16,
    },
    Midi2NoteOff {
        group: u8,
        channel: u8,
        note: u8,
        velocity: u16,
        attribute_type: u8,
        attribute_data: u16,
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
    Midi2RegisteredController {
        group: u8,
        channel: u8,
        bank: u8,
        index: u8,
        value: u32,
    },
    Midi2AssignableController {
        group: u8,
        channel: u8,
        bank: u8,
        index: u8,
        value: u32,
    },
    Midi2RegisteredControllerRelative {
        group: u8,
        channel: u8,
        bank: u8,
        index: u8,
        delta: i32,
    },
    Midi2AssignableControllerRelative {
        group: u8,
        channel: u8,
        bank: u8,
        index: u8,
        delta: i32,
    },
    Midi2PerNotePitchBend {
        group: u8,
        channel: u8,
        note: u8,
        value: u32,
    },
    Midi2RegisteredPerNote {
        group: u8,
        channel: u8,
        note: u8,
        index: u8,
        value: u32,
    },
    Midi2AssignablePerNote {
        group: u8,
        channel: u8,
        note: u8,
        index: u8,
        value: u32,
    },
    Midi2PerNoteManagement {
        group: u8,
        channel: u8,
        note: u8,
        flags: u8,
    },
    Noop,
    JrClock {
        ticks: u16,
    },
    JrTimestamp {
        ticks: u16,
    },
    Dctpq {
        ticks_per_qn: u16,
    },
    DeltaClockstamp {
        ticks: u32,
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
            Self::SongPosition { beats, .. } => format!("SongPos {beats}"),
            Self::MtcQuarter { data, .. } => format!("MTC QF {data:02X}"),
            Self::Sysex7 { count, .. } => format!("SysEx7 {count} bytes"),
            Self::SysEx8 {
                count, stream_id, ..
            } => format!("SysEx8 {count} bytes stream {stream_id}"),
            Self::MixData { status, mds_id, .. } => {
                format!("MixData status {status:#X} mds {mds_id}")
            }
            Self::Midi2NoteOn {
                channel,
                note,
                velocity,
                attribute_type,
                attribute_data,
                ..
            } => midi2_note_summary(
                "NoteOn",
                channel,
                note,
                velocity,
                attribute_type,
                attribute_data,
            ),
            Self::Midi2NoteOff {
                channel,
                note,
                velocity,
                attribute_type,
                attribute_data,
                ..
            } => midi2_note_summary(
                "NoteOff",
                channel,
                note,
                velocity,
                attribute_type,
                attribute_data,
            ),
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
            Self::Midi2RegisteredController {
                channel,
                bank,
                index,
                value,
                ..
            } => format!("Ch{} M2 RC bank {bank} idx {index} {value}", channel + 1),
            Self::Midi2AssignableController {
                channel,
                bank,
                index,
                value,
                ..
            } => format!("Ch{} M2 AC bank {bank} idx {index} {value}", channel + 1),
            Self::Midi2RegisteredControllerRelative {
                channel,
                bank,
                index,
                delta,
                ..
            } => format!(
                "Ch{} M2 RC rel bank {bank} idx {index} {delta}",
                channel + 1
            ),
            Self::Midi2AssignableControllerRelative {
                channel,
                bank,
                index,
                delta,
                ..
            } => format!(
                "Ch{} M2 AC rel bank {bank} idx {index} {delta}",
                channel + 1
            ),
            Self::Midi2PerNotePitchBend {
                channel,
                note,
                value,
                ..
            } => format!("Ch{} M2 PN Bend note {note} {value}", channel + 1),
            Self::Midi2RegisteredPerNote {
                channel,
                note,
                index,
                value,
                ..
            } => format!("Ch{} M2 PN RC note {note} idx {index} {value}", channel + 1),
            Self::Midi2AssignablePerNote {
                channel,
                note,
                index,
                value,
                ..
            } => format!("Ch{} M2 PN AC note {note} idx {index} {value}", channel + 1),
            Self::Midi2PerNoteManagement {
                channel,
                note,
                flags,
                ..
            } => format!(
                "Ch{} M2 PN Mgmt note {note} flags {flags:#04X}",
                channel + 1
            ),
            Self::Noop => "NOOP".to_string(),
            Self::JrClock { ticks } => format!("JR Clock {ticks}"),
            Self::JrTimestamp { ticks } => format!("JR Timestamp {ticks}"),
            Self::Dctpq { ticks_per_qn } => format!("DCTPQ {ticks_per_qn}"),
            Self::DeltaClockstamp { ticks } => format!("Delta Clockstamp {ticks}"),
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
            Self::SongPosition { .. } => "song_position",
            Self::MtcQuarter { .. } => "mtc",
            Self::Sysex7 { .. } => "sysex",
            Self::SysEx8 { .. } => "sysex8",
            Self::MixData { .. } => "mixdata",
            Self::Midi2NoteOn { .. } => "m2_note_on",
            Self::Midi2NoteOff { .. } => "m2_note_off",
            Self::Midi2ControlChange { .. } => "m2_cc",
            Self::Midi2PolyPressure { .. } => "m2_poly_pressure",
            Self::Midi2ProgramChange { .. } => "m2_program",
            Self::Midi2ChannelPressure { .. } => "m2_channel_pressure",
            Self::Midi2PitchBend { .. } => "m2_pitch_bend",
            Self::Midi2RegisteredController { .. } => "m2_rc",
            Self::Midi2AssignableController { .. } => "m2_ac",
            Self::Midi2RegisteredControllerRelative { .. } => "m2_rc_rel",
            Self::Midi2AssignableControllerRelative { .. } => "m2_ac_rel",
            Self::Midi2PerNotePitchBend { .. } => "m2_pn_bend",
            Self::Midi2RegisteredPerNote { .. } => "m2_pn_rc",
            Self::Midi2AssignablePerNote { .. } => "m2_pn_ac",
            Self::Midi2PerNoteManagement { .. } => "m2_pn_mgmt",
            Self::Noop => "noop",
            Self::JrClock { .. } => "jr_clock",
            Self::JrTimestamp { .. } => "jr_timestamp",
            Self::Dctpq { .. } => "dctpq",
            Self::DeltaClockstamp { .. } => "delta_clockstamp",
            Self::Other { .. } => "other",
        }
    }
}

fn midi2_note_summary(
    kind: &str,
    channel: u8,
    note: u8,
    velocity: u16,
    attribute_type: u8,
    attribute_data: u16,
) -> String {
    let mut s = format!("Ch{} M2 {kind} {note} vel16 {velocity}", channel + 1);
    if attribute_type != 0 {
        s.push_str(&format!(" attr {attribute_type} {attribute_data:#06X}"));
    }
    s
}

pub fn decode(msg: &UmpMessage) -> Decoded {
    let group = msg.group();
    match msg.message_type() {
        0x0 => decode_utility(msg),
        0x1 => decode_system(msg),
        0x2 => decode_midi1_channel(group, msg.words()[0]),
        0x3 => decode_sysex7(group, msg),
        0x4 => decode_midi2_channel(msg),
        0x5 => decode_data64(group, msg),
        mt => Decoded::Other {
            message_type: mt,
            group,
            status: msg.status_byte(),
        },
    }
}

fn decode_utility(msg: &UmpMessage) -> Decoded {
    let word0 = msg.words()[0];
    let ticks16 = (word0 & 0xFFFF) as u16;
    match msg.status_byte() >> 4 {
        0x0 => Decoded::Noop,
        0x1 => Decoded::JrClock { ticks: ticks16 },
        0x2 => Decoded::JrTimestamp { ticks: ticks16 },
        0x3 => Decoded::Dctpq {
            ticks_per_qn: ticks16,
        },
        0x4 => Decoded::DeltaClockstamp {
            ticks: word0 & 0xF_FFFF,
        },
        _ => Decoded::Other {
            message_type: 0x0,
            group: msg.group(),
            status: msg.status_byte(),
        },
    }
}

fn decode_system(msg: &UmpMessage) -> Decoded {
    let group = msg.group();
    match msg.status_byte() {
        0xF8 => Decoded::Clock { group },
        0xFA => Decoded::Start { group },
        0xFB => Decoded::Continue { group },
        0xFC => Decoded::Stop { group },
        0xF1 => Decoded::MtcQuarter {
            group,
            data: msg.data1(),
        },
        0xF2 => Decoded::SongPosition {
            group,
            beats: u16::from(msg.data1() & 0x7F) | (u16::from(msg.data2() & 0x7F) << 7),
        },
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
            attribute_type: msg.data2(),
            attribute_data: (w1 & 0xFFFF) as u16,
        },
        0x80 => Decoded::Midi2NoteOff {
            group,
            channel,
            note: d1,
            velocity: (w1 >> 16) as u16,
            attribute_type: msg.data2(),
            attribute_data: (w1 & 0xFFFF) as u16,
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
        0x20 => Decoded::Midi2RegisteredController {
            group,
            channel,
            bank: d1,
            index: msg.data2(),
            value: w1,
        },
        0x30 => Decoded::Midi2AssignableController {
            group,
            channel,
            bank: d1,
            index: msg.data2(),
            value: w1,
        },
        0x40 => Decoded::Midi2RegisteredControllerRelative {
            group,
            channel,
            bank: d1,
            index: msg.data2(),
            delta: w1 as i32,
        },
        0x50 => Decoded::Midi2AssignableControllerRelative {
            group,
            channel,
            bank: d1,
            index: msg.data2(),
            delta: w1 as i32,
        },
        0x60 => Decoded::Midi2PerNotePitchBend {
            group,
            channel,
            note: d1,
            value: w1,
        },
        0x00 => Decoded::Midi2RegisteredPerNote {
            group,
            channel,
            note: d1,
            index: msg.data2(),
            value: w1,
        },
        0x10 => Decoded::Midi2AssignablePerNote {
            group,
            channel,
            note: d1,
            index: msg.data2(),
            value: w1,
        },
        0xF0 => Decoded::Midi2PerNoteManagement {
            group,
            channel,
            note: d1,
            flags: msg.data2(),
        },
        _ => Decoded::Other {
            message_type: 0x4,
            group,
            status,
        },
    }
}

fn decode_data64(group: u8, msg: &UmpMessage) -> Decoded {
    let w0 = msg.words()[0];
    let status = ((w0 >> 20) & 0xF) as u8;
    match status {
        0..=3 => {
            let count = ((w0 >> 16) & 0xF) as u8;
            let stream_id = ((w0 >> 8) & 0xFF) as u8;
            let w1 = msg.words().get(1).copied().unwrap_or(0);
            let w2 = msg.words().get(2).copied().unwrap_or(0);
            let w3 = msg.words().get(3).copied().unwrap_or(0);
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
            Decoded::SysEx8 {
                group,
                status,
                stream_id,
                count,
                data,
            }
        }
        0x8..=0xB => Decoded::MixData {
            group,
            status,
            mds_id: ((w0 >> 16) & 0xF) as u8,
        },
        _ => Decoded::Other {
            message_type: 0x5,
            group,
            status: msg.status_byte(),
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
    fn decodes_spp_and_mtc() {
        let spp = UmpMessage::midi1_system(0, 0xF2, 0x10, 0x00);
        assert_eq!(decode(&spp).summary(), "SongPos 16");
        let mtc = UmpMessage::midi1_system(0, 0xF1, 0x21, 0);
        assert!(decode(&mtc).summary().contains("MTC"));
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
                velocity: 0x8000,
                attribute_type: 0,
                attribute_data: 0,
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

    #[test]
    fn decodes_midi2_per_note_and_controllers() {
        use crate::midi2::{
            midi2_assignable_controller, midi2_per_note_pitch_bend, midi2_registered_controller,
            midi2_registered_per_note,
        };
        let rc = midi2_registered_controller(0, 0, 0, 6, 0x8000_0000);
        assert_eq!(decode(&rc).kind_key(), "m2_rc");
        assert!(decode(&rc).summary().contains("M2 RC"));
        let ac = midi2_assignable_controller(0, 1, 2, 3, 1);
        assert_eq!(decode(&ac).kind_key(), "m2_ac");
        let pnb = midi2_per_note_pitch_bend(0, 4, 60, 0x8000_0000);
        assert_eq!(decode(&pnb).kind_key(), "m2_pn_bend");
        let pnrc = midi2_registered_per_note(0, 0, 64, 7, 99);
        assert_eq!(decode(&pnrc).kind_key(), "m2_pn_rc");
    }

    #[test]
    fn jr_timestamp_decodes() {
        let w = 0x0020_0100u32; // mt 0, status 2, jr 0x0100
        let m = UmpMessage::from_word(w).unwrap();
        assert_eq!(crate::decode(&m).kind_key(), "jr_timestamp");
    }

    #[test]
    fn utility_messages_decode() {
        let noop = UmpMessage::from_word(0x0000_0000).unwrap();
        assert_eq!(decode(&noop), Decoded::Noop);
        assert_eq!(decode(&noop).kind_key(), "noop");
        assert_eq!(decode(&noop).summary(), "NOOP");

        let clock = UmpMessage::from_word(0x0010_00AB).unwrap();
        assert_eq!(decode(&clock), Decoded::JrClock { ticks: 0x00AB });
        assert_eq!(decode(&clock).kind_key(), "jr_clock");
        assert_eq!(decode(&clock).summary(), "JR Clock 171");

        let ts = UmpMessage::from_word(0x0020_0100).unwrap();
        assert_eq!(decode(&ts), Decoded::JrTimestamp { ticks: 0x0100 });
        assert_eq!(decode(&ts).summary(), "JR Timestamp 256");

        let dctpq = UmpMessage::from_word(0x0030_01E0).unwrap();
        assert_eq!(decode(&dctpq), Decoded::Dctpq { ticks_per_qn: 480 });
        assert_eq!(decode(&dctpq).kind_key(), "dctpq");
        assert_eq!(decode(&dctpq).summary(), "DCTPQ 480");

        let dc = UmpMessage::from_word(0x0040_0010).unwrap();
        assert_eq!(decode(&dc), Decoded::DeltaClockstamp { ticks: 16 });
        assert_eq!(decode(&dc).kind_key(), "delta_clockstamp");
        assert_eq!(decode(&dc).summary(), "Delta Clockstamp 16");
    }

    #[test]
    fn unknown_utility_status_is_other() {
        let m = UmpMessage::from_word(0x0050_0001).unwrap();
        assert_eq!(
            decode(&m),
            Decoded::Other {
                message_type: 0x0,
                group: 0,
                status: 0x50
            }
        );
    }

    #[test]
    fn utility_constructors_roundtrip() {
        use crate::{ump_dctpq, ump_delta_clockstamp, ump_jr_clock, ump_jr_timestamp, ump_noop};

        let noop = ump_noop();
        assert_eq!(noop.message_type(), 0x0);
        assert_eq!(noop.status_byte(), 0x00);
        assert_eq!(noop.words()[0], 0x0000_0000);
        assert_eq!(decode(&noop), Decoded::Noop);

        let clock = ump_jr_clock(0x1234);
        assert_eq!(clock.words()[0], 0x0010_1234);
        assert_eq!(clock.status_byte(), 0x10);
        assert_eq!(decode(&clock), Decoded::JrClock { ticks: 0x1234 });

        let ts = ump_jr_timestamp(0x0100);
        assert_eq!(ts.words()[0], 0x0020_0100);
        assert_eq!(ts.status_byte(), 0x20);
        assert_eq!(decode(&ts), Decoded::JrTimestamp { ticks: 0x0100 });
        assert_eq!(decode(&ts).kind_key(), "jr_timestamp");

        let dctpq = ump_dctpq(480);
        assert_eq!(dctpq.words()[0], 0x0030_01E0);
        assert_eq!(decode(&dctpq), Decoded::Dctpq { ticks_per_qn: 480 });

        let dc = ump_delta_clockstamp(16);
        assert_eq!(dc.words()[0], 0x0040_0010);
        assert_eq!(decode(&dc), Decoded::DeltaClockstamp { ticks: 16 });
    }

    #[test]
    fn delta_clockstamp_decodes_20_bit_ticks() {
        // UMP 1.1 DC: bits 31–28 MT 0, 27–24 0, 23–20 status 4, 19–0 ticks.
        // ticks 0x1_2345 → word 0x0041_2345 (status_byte is 0x41, not 0x40).
        let m = UmpMessage::from_word(0x0041_2345).unwrap();
        assert_eq!(decode(&m), Decoded::DeltaClockstamp { ticks: 0x12345 });
        assert_eq!(decode(&m).kind_key(), "delta_clockstamp");
        assert_eq!(decode(&m).summary(), "Delta Clockstamp 74565");

        let dc = crate::ump_delta_clockstamp(0x1_2345);
        assert_eq!(dc.words()[0], 0x0041_2345);
        assert_eq!(decode(&dc), Decoded::DeltaClockstamp { ticks: 0x12345 });

        // Constructor masks to 20 bits.
        let masked = crate::ump_delta_clockstamp(0xF1_2345);
        assert_eq!(masked.words()[0], 0x0041_2345);
    }

    #[test]
    fn utility_decode_matches_status_nibble() {
        // Reserved bits 19–16 must not break 16-bit JR/DCTPQ payloads.
        let clock = UmpMessage::from_word(0x001F_00AB).unwrap();
        assert_eq!(decode(&clock), Decoded::JrClock { ticks: 0x00AB });
        let ts = UmpMessage::from_word(0x002A_0100).unwrap();
        assert_eq!(decode(&ts), Decoded::JrTimestamp { ticks: 0x0100 });
        let dctpq = UmpMessage::from_word(0x0031_01E0).unwrap();
        assert_eq!(decode(&dctpq), Decoded::Dctpq { ticks_per_qn: 480 });
    }

    #[test]
    fn decodes_sysex8_kind_key() {
        let m = UmpMessage::try_from_words(&[0x501E_AB00, 0x0102_0304, 0x0506_0708, 0x090A_0B0C])
            .unwrap();
        match decode(&m) {
            Decoded::SysEx8 {
                group,
                status,
                stream_id,
                count,
                data,
            } => {
                assert_eq!(group, 0);
                assert_eq!(status, 1);
                assert_eq!(stream_id, 0xAB);
                assert_eq!(count, 14);
                assert_eq!(&data, &(0u8..13).collect::<Vec<_>>()[..]);
            }
            other => panic!("expected SysEx8, got {other:?}"),
        }
        assert_eq!(decode(&m).kind_key(), "sysex8");
        assert!(decode(&m).summary().contains("SysEx8"));
    }

    #[test]
    fn decodes_mixdata_kind_key() {
        let m = UmpMessage::try_from_words(&[0x5285_0000, 0, 0, 0]).unwrap();
        match decode(&m) {
            Decoded::MixData {
                group,
                status,
                mds_id,
            } => {
                assert_eq!(group, 2);
                assert_eq!(status, 8);
                assert_eq!(mds_id, 5);
            }
            other => panic!("expected MixData, got {other:?}"),
        }
        assert_eq!(decode(&m).kind_key(), "mixdata");
        assert!(decode(&m).summary().contains("MixData"));
    }
}
