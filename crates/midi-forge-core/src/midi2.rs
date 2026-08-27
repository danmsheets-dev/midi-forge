use crate::ump::UmpMessage;

/// 16-bit MIDI 2 velocity → 7-bit MIDI 1. Zero stays zero; nonzero maps to 1–127.
pub fn velocity16_to_7(velocity: u16) -> u8 {
    if velocity == 0 {
        0
    } else {
        ((u32::from(velocity) * 127 + 32767) / 65535).clamp(1, 127) as u8
    }
}

/// 32-bit MIDI 2 controller value → 7-bit.
pub fn value32_to_7(value: u32) -> u8 {
    (value >> 25) as u8
}

/// 32-bit MIDI 2 pitch bend → 14-bit MIDI 1 (lsb + msb).
pub fn pitch32_to_14(value: u32) -> (u8, u8) {
    let v14 = (value >> 18) as u16;
    ((v14 & 0x7F) as u8, ((v14 >> 7) & 0x7F) as u8)
}

/// 7-bit → 16-bit velocity. Zero stays zero; 127 becomes 65535.
pub fn velocity7_to_16(velocity: u8) -> u16 {
    if velocity == 0 {
        0
    } else {
        (u32::from(velocity) * 65535 / 127) as u16
    }
}

/// 7-bit → 32-bit controller (matches `value32_to_7` as a round-trip for 0 and 127).
pub fn value7_to_32(value: u8) -> u32 {
    u32::from(value.min(127)) << 25
}

pub fn midi2_note_on(group: u8, channel: u8, note: u8, velocity: u16) -> UmpMessage {
    midi2_note_on_attr(group, channel, note, velocity, 0, 0)
}

/// MIDI 2 Note On. `attr_type`: 0 none, 1 manufacturer, 2 profile, 3 pitch 7.9.
pub fn midi2_note_on_attr(
    group: u8,
    channel: u8,
    note: u8,
    velocity: u16,
    attr_type: u8,
    attr_data: u16,
) -> UmpMessage {
    UmpMessage::midi2_channel_voice(
        group,
        0x90 | (channel & 0x0F),
        note,
        attr_type,
        (u32::from(velocity) << 16) | u32::from(attr_data),
    )
}

pub fn midi2_note_off(group: u8, channel: u8, note: u8, velocity: u16) -> UmpMessage {
    midi2_note_off_attr(group, channel, note, velocity, 0, 0)
}

/// MIDI 2 Note Off. Attribute fields match [`midi2_note_on_attr`].
pub fn midi2_note_off_attr(
    group: u8,
    channel: u8,
    note: u8,
    velocity: u16,
    attr_type: u8,
    attr_data: u16,
) -> UmpMessage {
    UmpMessage::midi2_channel_voice(
        group,
        0x80 | (channel & 0x0F),
        note,
        attr_type,
        (u32::from(velocity) << 16) | u32::from(attr_data),
    )
}

pub fn midi2_cc(group: u8, channel: u8, controller: u8, value: u32) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0xB0 | (channel & 0x0F), controller, 0, value)
}

pub fn midi2_pitch_bend(group: u8, channel: u8, value: u32) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0xE0 | (channel & 0x0F), 0, 0, value)
}

/// MIDI 2 Registered Controller (replaces RPN). `bank`/`index` are 7-bit.
pub fn midi2_registered_controller(
    group: u8,
    channel: u8,
    bank: u8,
    index: u8,
    value: u32,
) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0x20 | (channel & 0x0F), bank, index, value)
}

/// MIDI 2 Assignable Controller (replaces NRPN).
pub fn midi2_assignable_controller(
    group: u8,
    channel: u8,
    bank: u8,
    index: u8,
    value: u32,
) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0x30 | (channel & 0x0F), bank, index, value)
}

pub fn midi2_per_note_pitch_bend(group: u8, channel: u8, note: u8, value: u32) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0x60 | (channel & 0x0F), note, 0, value)
}

pub fn midi2_registered_per_note(
    group: u8,
    channel: u8,
    note: u8,
    index: u8,
    value: u32,
) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0x00 | (channel & 0x0F), note, index, value)
}

pub fn midi2_assignable_per_note(
    group: u8,
    channel: u8,
    note: u8,
    index: u8,
    value: u32,
) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0x10 | (channel & 0x0F), note, index, value)
}

pub fn midi2_per_note_management(group: u8, channel: u8, note: u8, flags: u8) -> UmpMessage {
    UmpMessage::midi2_channel_voice(group, 0xF0 | (channel & 0x0F), note, flags, 0)
}

/// If `packet` is MIDI 2.0 channel voice, return equivalent MIDI 1.0 UMP(s).
/// Program Change with Bank Valid emits CC0, CC32, then PC. Other types pass through.
pub fn downscale_to_midi1(packet: &UmpMessage) -> Vec<UmpMessage> {
    if packet.message_type() != 0x4 || packet.len() < 2 {
        return vec![*packet];
    }
    let status = packet.status_byte();
    let group = packet.group();
    let d1 = packet.data1();
    let w1 = packet.words()[1];
    let one = |s, a, b| vec![UmpMessage::midi1_channel_voice(group, s, a, b)];
    match status & 0xF0 {
        0x80 => {
            let vel16 = (w1 >> 16) as u16;
            one(status, d1, velocity16_to_7(vel16))
        }
        0x90 => {
            // M2-104-UM D.2.1: MIDI 2 Note On velocity 0 must not become MIDI 1 Note On vel 0 (Note Off).
            let vel16 = (w1 >> 16) as u16;
            let vel7 = velocity16_to_7(vel16).max(1);
            one(status, d1, vel7)
        }
        0xA0 => one(status, d1, value32_to_7(w1)),
        0xB0 => one(status, d1, value32_to_7(w1)),
        0xC0 => {
            let program = ((w1 >> 24) & 0x7F) as u8;
            let pc = UmpMessage::midi1_channel_voice(group, status, program, 0);
            if d1 & 0x01 == 0 {
                return vec![pc];
            }
            let ch = status & 0x0F;
            let bank_msb = ((w1 >> 8) & 0x7F) as u8;
            let bank_lsb = (w1 & 0x7F) as u8;
            vec![
                UmpMessage::midi1_channel_voice(group, 0xB0 | ch, 0, bank_msb),
                UmpMessage::midi1_channel_voice(group, 0xB0 | ch, 32, bank_lsb),
                pc,
            ]
        }
        0xD0 => one(status, value32_to_7(w1), 0),
        0xE0 => {
            let (lsb, msb) = pitch32_to_14(w1);
            one(status, lsb, msb)
        }
        0x20 => rpn_nrpn(group, status & 0x0F, 101, 100, d1, packet.data2(), w1),
        0x30 => rpn_nrpn(group, status & 0x0F, 99, 98, d1, packet.data2(), w1),
        // Per-note and relative controllers have no MIDI 1.0 short-message form.
        0x00 | 0x10 | 0x40 | 0x50 | 0x60 | 0xF0 => Vec::new(),
        _ => vec![*packet],
    }
}

fn rpn_nrpn(
    group: u8,
    ch: u8,
    msb_cc: u8,
    lsb_cc: u8,
    bank: u8,
    index: u8,
    value: u32,
) -> Vec<UmpMessage> {
    let cc = |num, val| UmpMessage::midi1_channel_voice(group, 0xB0 | ch, num, val);
    vec![
        cc(msb_cc, bank & 0x7F),
        cc(lsb_cc, index & 0x7F),
        cc(6, value32_to_7(value)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn velocity_zero_stays_zero() {
        assert_eq!(velocity16_to_7(0), 0);
        assert_eq!(velocity16_to_7(65535), 127);
        assert!(velocity16_to_7(1) >= 1);
    }

    #[test]
    fn midi2_note_on_zero_velocity_downscales_to_one() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0x90, 60, 0, 0);
        let m1 = &downscale_to_midi1(&m2)[0];
        assert_eq!(m1.status_byte(), 0x90);
        assert_eq!(m1.data2(), 1);
        let off = UmpMessage::midi2_channel_voice(0, 0x80, 60, 0, 0);
        assert_eq!(downscale_to_midi1(&off)[0].data2(), 0);
    }

    #[test]
    fn note_on_downscale() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0x90, 60, 0, 0xFFFF_0000);
        let m1 = &downscale_to_midi1(&m2)[0];
        assert_eq!(m1.message_type(), 0x2);
        assert_eq!(m1.status_byte(), 0x90);
        assert_eq!(m1.data1(), 60);
        assert_eq!(m1.data2(), 127);
    }

    #[test]
    fn midi1_passthrough() {
        let m1 = UmpMessage::midi1_channel_voice(0, 0x90, 10, 20);
        assert_eq!(downscale_to_midi1(&m1), vec![m1]);
    }

    #[test]
    fn cc_downscale_uses_top_bits() {
        let m2 = UmpMessage::midi2_channel_voice(2, 0xB3, 7, 0, 0x8000_0000);
        let m1 = &downscale_to_midi1(&m2)[0];
        assert_eq!(m1.group(), 2);
        assert_eq!(m1.status_byte(), 0xB3);
        assert_eq!(m1.data1(), 7);
        assert_eq!(m1.data2(), 64);
    }

    #[test]
    fn pitch_center_downscales_to_midi1_center() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0xE0, 0, 0, 0x8000_0000);
        let m1 = &downscale_to_midi1(&m2)[0];
        assert_eq!(m1.data1(), 0);
        assert_eq!(m1.data2(), 64);
        assert_eq!(crate::packed_short_from_ump(m1), Some(0x40_00_E0));
    }

    #[test]
    fn program_change_without_bank_is_one_packet() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0xC2, 0, 0, 0x0C00_0000);
        let out = downscale_to_midi1(&m2);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].status_byte(), 0xC2);
        assert_eq!(out[0].data1(), 12);
    }

    #[test]
    fn program_change_bank_valid_emits_cc0_cc32_pc() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0xC2, 1, 0, 0x0C00_0503);
        let out = downscale_to_midi1(&m2);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].status_byte(), 0xB2);
        assert_eq!(out[0].data1(), 0);
        assert_eq!(out[0].data2(), 5);
        assert_eq!(out[1].data1(), 32);
        assert_eq!(out[1].data2(), 3);
        assert_eq!(out[2].status_byte(), 0xC2);
        assert_eq!(out[2].data1(), 12);
    }

    #[test]
    fn registered_controller_downscales_to_rpn() {
        let m2 = midi2_registered_controller(0, 3, 0, 6, 0x8000_0000);
        let out = downscale_to_midi1(&m2);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].status_byte(), 0xB3);
        assert_eq!(out[0].data1(), 101);
        assert_eq!(out[0].data2(), 0);
        assert_eq!(out[1].data1(), 100);
        assert_eq!(out[1].data2(), 6);
        assert_eq!(out[2].data1(), 6);
        assert_eq!(out[2].data2(), 64);
    }

    #[test]
    fn per_note_pitch_bend_drops_on_midi1() {
        let m2 = midi2_per_note_pitch_bend(0, 1, 60, 0x8000_0000);
        assert!(downscale_to_midi1(&m2).is_empty());
    }

    #[test]
    fn midi2_note_on_constructor_velocity() {
        let m2 = midi2_note_on(0, 2, 64, 0x8000);
        assert_eq!(m2.message_type(), 0x4);
        assert_eq!(m2.status_byte(), 0x92);
        assert_eq!(m2.data1(), 64);
        assert_eq!(m2.words()[1] >> 16, 0x8000);
        assert_eq!(m2.data2(), 0);
        assert_eq!(m2.words()[1] & 0xFFFF, 0);
    }

    #[test]
    fn note_on_attribute_roundtrip() {
        let m = midi2_note_on_attr(0, 1, 60, 0x8000, 3, 0x1234);
        assert_eq!(m.data2(), 3);
        assert_eq!(m.words()[1] & 0xFFFF, 0x1234);
        match crate::decode(&m) {
            crate::Decoded::Midi2NoteOn {
                attribute_type,
                attribute_data,
                velocity,
                ..
            } => {
                assert_eq!(attribute_type, 3);
                assert_eq!(attribute_data, 0x1234);
                assert_eq!(velocity, 0x8000);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            crate::decode(&m).summary(),
            "Ch2 M2 NoteOn 60 vel16 32768 attr 3 0x1234"
        );
    }

    #[test]
    fn note_off_attribute_roundtrip() {
        let m = midi2_note_off_attr(0, 1, 60, 0x4000, 1, 0xABCD);
        assert_eq!(m.message_type(), 0x4);
        assert_eq!(m.status_byte(), 0x81);
        assert_eq!(m.data1(), 60);
        assert_eq!(m.data2(), 1);
        assert_eq!(m.words()[1] >> 16, 0x4000);
        assert_eq!(m.words()[1] & 0xFFFF, 0xABCD);
        match crate::decode(&m) {
            crate::Decoded::Midi2NoteOff {
                attribute_type,
                attribute_data,
                velocity,
                ..
            } => {
                assert_eq!(attribute_type, 1);
                assert_eq!(attribute_data, 0xABCD);
                assert_eq!(velocity, 0x4000);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            crate::decode(&m).summary(),
            "Ch2 M2 NoteOff 60 vel16 16384 attr 1 0xABCD"
        );
    }

    #[test]
    fn note_attribute_ignored_on_downscale() {
        let on = midi2_note_on_attr(0, 1, 60, 0xFFFF, 3, 0x1234);
        let m1 = &downscale_to_midi1(&on)[0];
        assert_eq!(m1.message_type(), 0x2);
        assert_eq!(m1.status_byte(), 0x91);
        assert_eq!(m1.data1(), 60);
        assert_eq!(m1.data2(), 127);

        let off = midi2_note_off_attr(0, 1, 60, 0x8000, 2, 0x00FF);
        let m1 = &downscale_to_midi1(&off)[0];
        assert_eq!(m1.message_type(), 0x2);
        assert_eq!(m1.status_byte(), 0x81);
        assert_eq!(m1.data1(), 60);
        assert_eq!(m1.data2(), velocity16_to_7(0x8000));
    }

    #[test]
    fn value7_roundtrip_top_bits() {
        assert_eq!(value32_to_7(value7_to_32(0)), 0);
        assert_eq!(value32_to_7(value7_to_32(127)), 127);
        assert_eq!(value32_to_7(value7_to_32(64)), 64);
    }
}
