use crate::ump::UmpMessage;

/// Engine-assigned port handle. The IO crate maps OS endpoint ids onto these.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct PortId(pub u32);

/// Nanoseconds from a backend epoch (usually converted from a monotonic clock
/// at capture). Compare only values from the same backend session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Default)]
pub struct Timestamp {
    pub nanos: u64,
}

impl Timestamp {
    pub const fn from_nanos(nanos: u64) -> Self {
        Self { nanos }
    }
}

/// One captured UMP packet with timing and source port.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiEvent {
    pub time: Timestamp,
    pub port: PortId,
    pub packet: UmpMessage,
}

impl MidiEvent {
    pub fn new(time: Timestamp, port: PortId, packet: UmpMessage) -> Self {
        Self { time, port, packet }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ump::UmpMessage;

    #[test]
    fn event_holds_packet_and_port() {
        let packet = UmpMessage::midi1_channel_voice(0, 0x90, 60, 127);
        let ev = MidiEvent::new(Timestamp::from_nanos(1_000), PortId(2), packet);
        assert_eq!(ev.port, PortId(2));
        assert_eq!(ev.time.nanos, 1_000);
        assert_eq!(ev.packet, packet);
    }
}
