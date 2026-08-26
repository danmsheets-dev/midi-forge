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

/// If `packet` is MIDI 2.0 channel voice, return an equivalent MIDI 1.0 UMP.
/// Other types are returned unchanged.
pub fn downscale_to_midi1(packet: &UmpMessage) -> UmpMessage {
    if packet.message_type() != 0x4 || packet.len() < 2 {
        return *packet;
    }
    let status = packet.status_byte();
    let group = packet.group();
    let d1 = packet.data1();
    let w1 = packet.words()[1];
    match status & 0xF0 {
        0x80 | 0x90 => {
            let vel16 = (w1 >> 16) as u16;
            UmpMessage::midi1_channel_voice(group, status, d1, velocity16_to_7(vel16))
        }
        0xA0 => {
            let pressure = value32_to_7(w1);
            UmpMessage::midi1_channel_voice(group, status, d1, pressure)
        }
        0xB0 => UmpMessage::midi1_channel_voice(group, status, d1, value32_to_7(w1)),
        0xC0 => UmpMessage::midi1_channel_voice(group, status, ((w1 >> 24) & 0x7F) as u8, 0),
        0xD0 => UmpMessage::midi1_channel_voice(group, status, value32_to_7(w1), 0),
        0xE0 => {
            let (lsb, msb) = pitch32_to_14(w1);
            UmpMessage::midi1_channel_voice(group, status, lsb, msb)
        }
        _ => *packet,
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
        let m1 = downscale_to_midi1(&m2);
        assert_eq!(m1.message_type(), 0x2);
        assert_eq!(m1.status_byte(), 0x90);
        assert_eq!(m1.data1(), 60);
        assert_eq!(m1.data2(), 127);
    }

    #[test]
    fn midi1_passthrough() {
        let m1 = UmpMessage::midi1_channel_voice(0, 0x90, 10, 20);
        assert_eq!(downscale_to_midi1(&m1), m1);
    }

    #[test]
    fn cc_downscale_uses_top_bits() {
        let m2 = UmpMessage::midi2_channel_voice(2, 0xB3, 7, 0, 0x8000_0000);
        let m1 = downscale_to_midi1(&m2);
        assert_eq!(m1.group(), 2);
        assert_eq!(m1.status_byte(), 0xB3);
        assert_eq!(m1.data1(), 7);
        assert_eq!(m1.data2(), 64);
    }

    #[test]
    fn pitch_center_downscales_to_midi1_center() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0xE0, 0, 0, 0x8000_0000);
        let m1 = downscale_to_midi1(&m2);
        assert_eq!(m1.data1(), 0);
        assert_eq!(m1.data2(), 64);
        assert_eq!(crate::packed_short_from_ump(&m1), Some(0x40_00_E0));
    }

    #[test]
    fn program_change_uses_top_byte() {
        let m2 = UmpMessage::midi2_channel_voice(0, 0xC2, 1, 0, 0x0C00_0503);
        let m1 = downscale_to_midi1(&m2);
        assert_eq!(m1.status_byte(), 0xC2);
        assert_eq!(m1.data1(), 12);
        assert_eq!(m1.data2(), 0);
    }
}
