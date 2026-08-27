//! Host-side MIDI clock generator (24 PPQ).

use crate::ump::UmpMessage;

/// Generate MIDI clock, transport, and song position.
///
/// Intervals are host time, not 5-pin cable delay. `poll` must be called
/// from the engine thread at ~1 ms so 120 BPM (F8 every ~833 µs) stays honest.
#[derive(Clone, Debug)]
pub struct ClockMaster {
    pub enabled: bool,
    pub bpm: f64,
    running: bool,
    next_ns: u64,
    pub ticks: u64,
}

impl Default for ClockMaster {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockMaster {
    pub fn new() -> Self {
        Self {
            enabled: false,
            bpm: 120.0,
            running: false,
            next_ns: 0,
            ticks: 0,
        }
    }

    pub fn running(&self) -> bool {
        self.running
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(20.0, 400.0);
    }

    /// Nanoseconds between F8 ticks (24 per quarter note).
    pub fn interval_ns(&self) -> u64 {
        let bpm = self.bpm.clamp(20.0, 400.0);
        (60_000_000_000.0 / (bpm * 24.0)).round() as u64
    }

    pub fn start(&mut self, now_ns: u64) -> UmpMessage {
        self.enabled = true;
        self.running = true;
        self.next_ns = now_ns;
        self.ticks = 0;
        UmpMessage::midi1_system(0, 0xFA, 0, 0)
    }

    pub fn cont(&mut self, now_ns: u64) -> UmpMessage {
        self.enabled = true;
        self.running = true;
        self.next_ns = now_ns;
        UmpMessage::midi1_system(0, 0xFB, 0, 0)
    }

    pub fn stop(&mut self) -> UmpMessage {
        self.running = false;
        UmpMessage::midi1_system(0, 0xFC, 0, 0)
    }

    pub fn song_position(beats: u16) -> UmpMessage {
        let lsb = (beats & 0x7F) as u8;
        let msb = ((beats >> 7) & 0x7F) as u8;
        UmpMessage::midi1_system(0, 0xF2, lsb, msb)
    }

    /// Due F8 packets up to `now_ns`. Caps at 48 per call so a stall cannot flood.
    pub fn poll(&mut self, now_ns: u64) -> Vec<UmpMessage> {
        if !self.enabled || !self.running {
            return Vec::new();
        }
        let iv = self.interval_ns().max(1);
        let mut out = Vec::new();
        let mut n = 0u8;
        while self.next_ns <= now_ns && n < 48 {
            out.push(UmpMessage::midi1_system(0, 0xF8, 0, 0));
            self.next_ns = self.next_ns.saturating_add(iv);
            self.ticks += 1;
            n += 1;
        }
        if n == 48 {
            self.next_ns = now_ns.saturating_add(iv);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_120_bpm_is_24_ppq() {
        let mut m = ClockMaster::new();
        m.set_bpm(120.0);
        assert_eq!(m.interval_ns(), 20_833_333);
    }

    #[test]
    fn start_emits_fa_then_clocks() {
        let mut m = ClockMaster::new();
        m.set_bpm(120.0);
        let start = m.start(1_000);
        assert_eq!(start.status_byte(), 0xFA);
        assert!(m.running());
        let first = m.poll(1_000);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].status_byte(), 0xF8);
        let later = m.poll(1_000 + m.interval_ns());
        assert_eq!(later.len(), 1);
        assert_eq!(m.ticks, 2);
    }

    #[test]
    fn stop_halts_clocks() {
        let mut m = ClockMaster::new();
        m.start(0);
        let _ = m.poll(0);
        let stop = m.stop();
        assert_eq!(stop.status_byte(), 0xFC);
        assert!(!m.running());
        assert!(m.poll(1_000_000_000).is_empty());
    }

    #[test]
    fn catch_up_is_capped() {
        let mut m = ClockMaster::new();
        m.set_bpm(120.0);
        m.start(0);
        let flood = m.poll(u64::from(u32::MAX) * 10);
        assert_eq!(flood.len(), 48);
    }

    #[test]
    fn song_position_packs_14bit() {
        let p = ClockMaster::song_position(0x80);
        assert_eq!(p.status_byte(), 0xF2);
        assert_eq!(p.data1(), 0x00);
        assert_eq!(p.data2(), 0x01);
    }

    #[test]
    fn disabled_emits_nothing() {
        let mut m = ClockMaster::new();
        assert!(m.poll(1_000_000).is_empty());
    }
}
