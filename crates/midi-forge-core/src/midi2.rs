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
        0x80 | 0x90 => {
            let vel16 = (w1 >> 16) as u16;
            one(status, d1, velocity16_to_7(vel16))
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
        _ => vec![*packet],
    }
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
}
