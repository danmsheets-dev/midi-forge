use crate::ump::UmpMessage;

pub const ALL_SOUND_OFF: u8 = 120;
pub const RESET_ALL_CONTROLLERS: u8 = 121;
pub const ALL_NOTES_OFF: u8 = 123;

const PANIC_CCS: [u8; 3] = [ALL_SOUND_OFF, RESET_ALL_CONTROLLERS, ALL_NOTES_OFF];

/// MIDI panic: All Sound Off, Reset All Controllers, All Notes Off on every channel.
pub fn panic_packets() -> Vec<UmpMessage> {
    let mut out = Vec::with_capacity(16 * PANIC_CCS.len());
    for channel in 0..16 {
        let status = 0xB0 | channel;
        for cc in PANIC_CCS {
            out.push(UmpMessage::midi1_channel_voice(0, status, cc, 0));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoded;
    use crate::decode::decode;

    #[test]
    fn panic_covers_all_channels_in_order() {
        let packets = panic_packets();
        assert_eq!(packets.len(), 48);
        match decode(&packets[0]) {
            Decoded::ControlChange {
                channel,
                controller,
                value,
                ..
            } => {
                assert_eq!(channel, 0);
                assert_eq!(controller, ALL_SOUND_OFF);
                assert_eq!(value, 0);
            }
            other => panic!("unexpected {other:?}"),
        }
        match decode(&packets[47]) {
            Decoded::ControlChange {
                channel,
                controller,
                ..
            } => {
                assert_eq!(channel, 15);
                assert_eq!(controller, ALL_NOTES_OFF);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
