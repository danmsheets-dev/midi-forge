use crate::ump::UmpMessage;

/// Pack a 1-word UMP Utility message (type 0x0). Status lives in bits 23–20.
fn utility(status: u8, payload: u16) -> UmpMessage {
    let word = (u32::from(status) << 20) | u32::from(payload);
    UmpMessage::from_word(word).expect("UMP type 0 is one word")
}

pub fn ump_noop() -> UmpMessage {
    utility(0, 0)
}

/// JR Clock: 16-bit sender clock (1/31250 s).
pub fn ump_jr_clock(ticks: u16) -> UmpMessage {
    utility(1, ticks)
}

/// JR Timestamp: 16-bit sender clock (1/31250 s).
pub fn ump_jr_timestamp(ticks: u16) -> UmpMessage {
    utility(2, ticks)
}

/// Delta Clockstamp Ticks Per Quarter Note (clip timing).
pub fn ump_dctpq(ticks_per_qn: u16) -> UmpMessage {
    utility(3, ticks_per_qn)
}

/// Delta Clockstamp (clip timing). 20-bit tick count in bits 19–0.
pub fn ump_delta_clockstamp(ticks: u32) -> UmpMessage {
    let word = (0x4u32 << 20) | (ticks & 0xF_FFFF);
    UmpMessage::from_word(word).expect("UMP type 0 is one word")
}
