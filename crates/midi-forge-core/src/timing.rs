use std::collections::VecDeque;

use crate::midi2::downscale_to_midi1;
use crate::ump::UmpMessage;

const CLOCK_CAP: usize = 48;
const NOTE_CAP: usize = 64;

/// Rolling inter-arrival times in nanoseconds (host receive, not cable delay).
#[derive(Clone, Debug)]
pub struct IntervalHist {
    last_ns: Option<u64>,
    samples: VecDeque<u64>,
    cap: usize,
}

impl IntervalHist {
    pub fn new(cap: usize) -> Self {
        Self {
            last_ns: None,
            samples: VecDeque::with_capacity(cap.max(1)),
            cap: cap.max(1),
        }
    }

    pub fn push(&mut self, t_ns: u64) {
        if let Some(prev) = self.last_ns
            && t_ns > prev
        {
            self.samples.push_back(t_ns - prev);
            while self.samples.len() > self.cap {
                self.samples.pop_front();
            }
        }
        self.last_ns = Some(t_ns);
    }

    pub fn clear(&mut self) {
        self.last_ns = None;
        self.samples.clear();
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub fn mean_ns(&self) -> Option<u64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: u128 = self.samples.iter().map(|&x| u128::from(x)).sum();
        Some((sum / self.samples.len() as u128) as u64)
    }

    /// Population standard deviation of intervals.
    pub fn jitter_ns(&self) -> Option<u64> {
        let mean = self.mean_ns()?;
        if self.samples.len() < 2 {
            return Some(0);
        }
        let var: u128 = self
            .samples
            .iter()
            .map(|&x| {
                let d = i128::from(x) - i128::from(mean);
                (d * d) as u128
            })
            .sum::<u128>()
            / self.samples.len() as u128;
        Some((var as f64).sqrt() as u64)
    }

    /// MIDI clock is 24 PPQ. Needs at least one interval.
    pub fn bpm_from_midi_clock(&self) -> Option<f64> {
        let mean = self.mean_ns()? as f64;
        if mean < 1.0 {
            return None;
        }
        Some(60_000_000_000.0 / (mean * 24.0))
    }

    /// `n` equal-width bins over min..=max. Empty hist → empty vec.
    pub fn bins(&self, n: usize) -> Vec<usize> {
        let n = n.max(1);
        if self.samples.is_empty() {
            return vec![0; n];
        }
        let min = *self.samples.iter().min().unwrap();
        let max = *self.samples.iter().max().unwrap();
        let mut out = vec![0usize; n];
        if min == max {
            out[0] = self.samples.len();
            return out;
        }
        let span = max - min;
        for &s in &self.samples {
            let i = (((s - min) as u128 * (n as u128 - 1)) / u128::from(span)) as usize;
            out[i.min(n - 1)] += 1;
        }
        out
    }

    pub fn min_max_ns(&self) -> Option<(u64, u64)> {
        let min = *self.samples.iter().min()?;
        let max = *self.samples.iter().max()?;
        Some((min, max))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Transport {
    #[default]
    Stopped,
    Playing,
}

/// MIDI Time Code quarter-frame assembler (F1).
#[derive(Clone, Debug, Default)]
pub struct MtcState {
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    rate: u8,
    seen: u8,
}

impl MtcState {
    pub fn push_qf(&mut self, data: u8) {
        let nibble = data & 0x0F;
        let typ = (data >> 4) & 0x07;
        match typ {
            0 => self.frames = (self.frames & 0xF0) | nibble,
            1 => self.frames = (self.frames & 0x0F) | ((nibble & 0x01) << 4),
            2 => self.seconds = (self.seconds & 0xF0) | nibble,
            3 => self.seconds = (self.seconds & 0x0F) | ((nibble & 0x03) << 4),
            4 => self.minutes = (self.minutes & 0xF0) | nibble,
            5 => self.minutes = (self.minutes & 0x0F) | ((nibble & 0x03) << 4),
            6 => self.hours = (self.hours & 0xF0) | nibble,
            7 => {
                self.hours = (self.hours & 0x0F) | ((nibble & 0x01) << 4);
                self.rate = (nibble >> 1) & 0x03;
            }
            _ => {}
        }
        self.seen |= 1 << typ;
    }

    pub fn complete(&self) -> bool {
        self.seen == 0xFF
    }

    pub fn display(&self) -> Option<String> {
        if !self.complete() {
            return None;
        }
        let fps = match self.rate {
            0 => 24,
            1 => 25,
            2 => 29,
            _ => 30,
        };
        Some(format!(
            "{h:02}:{m:02}:{s:02}:{f:02} @{fps}",
            h = self.hours & 0x1F,
            m = self.minutes,
            s = self.seconds,
            f = self.frames
        ))
    }
}

/// Clock, transport, SPP, MTC, and note-on receive intervals.
#[derive(Clone, Debug)]
pub struct ClockHealth {
    pub clock: IntervalHist,
    pub notes: IntervalHist,
    pub transport: Transport,
    pub song_pos: Option<u16>,
    pub mtc: MtcState,
    pub clocks: u64,
}

impl Default for ClockHealth {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockHealth {
    pub fn new() -> Self {
        Self {
            clock: IntervalHist::new(CLOCK_CAP),
            notes: IntervalHist::new(NOTE_CAP),
            transport: Transport::Stopped,
            song_pos: None,
            mtc: MtcState::default(),
            clocks: 0,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn push(&mut self, packet: &UmpMessage, t_ns: u64) {
        if packet.message_type() == 0x4 {
            for p in downscale_to_midi1(packet) {
                self.push_midi1(&p, t_ns);
            }
            return;
        }
        self.push_midi1(packet, t_ns);
    }

    fn push_midi1(&mut self, packet: &UmpMessage, t_ns: u64) {
        match packet.message_type() {
            0x2 => {
                if packet.status_byte() & 0xF0 == 0x90 && packet.data2() > 0 {
                    self.notes.push(t_ns);
                }
            }
            0x1 => match packet.status_byte() {
                0xF8 => {
                    self.clocks += 1;
                    self.clock.push(t_ns);
                }
                0xFA => {
                    self.transport = Transport::Playing;
                    self.song_pos = Some(0);
                }
                0xFB => self.transport = Transport::Playing,
                0xFC => self.transport = Transport::Stopped,
                0xF2 => {
                    let beats =
                        u16::from(packet.data1() & 0x7F) | (u16::from(packet.data2() & 0x7F) << 7);
                    self.song_pos = Some(beats);
                }
                0xF1 => self.mtc.push_qf(packet.data1()),
                _ => {}
            },
            _ => {}
        }
    }

    /// Firehose / broken clock: mean interval under 2 ms, or BPM over 300.
    pub fn runaway(&self) -> bool {
        if let Some(bpm) = self.clock.bpm_from_midi_clock()
            && bpm > 300.0
        {
            return true;
        }
        self.clock.mean_ns().is_some_and(|m| m < 2_000_000)
    }

    pub fn spp_bars_4_4(&self) -> Option<String> {
        let pos = self.song_pos?;
        let bar = pos / 16 + 1;
        let sixteenth = pos % 16;
        let beat = sixteenth / 4 + 1;
        let tick = sixteenth % 4;
        Some(format!("{bar}.{beat}.{tick} ({pos} 16ths)"))
    }

    pub fn summary(&self) -> String {
        let tr = match self.transport {
            Transport::Playing => "playing",
            Transport::Stopped => "stopped",
        };
        let bpm = self
            .clock
            .bpm_from_midi_clock()
            .map(|b| format!("{b:.1}"))
            .unwrap_or_else(|| "—".into());
        let jit = self
            .clock
            .jitter_ns()
            .map(|j| format!("{} µs", j / 1000))
            .unwrap_or_else(|| "—".into());
        let mut s = format!("{tr}  {bpm} BPM  jitter {jit}  {n} clocks", n = self.clocks);
        if self.runaway() {
            s.push_str("  RUNAWAY");
        }
        if let Some(spp) = self.spp_bars_4_4() {
            s.push_str("  SPP ");
            s.push_str(&spp);
        }
        if let Some(mtc) = self.mtc.display() {
            s.push_str("  MTC ");
            s.push_str(&mtc);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock_at(ns: u64) -> (UmpMessage, u64) {
        (UmpMessage::midi1_system(0, 0xF8, 0, 0), ns)
    }

    #[test]
    fn bpm_120_from_clock() {
        let mut h = ClockHealth::new();
        // 120 BPM → 24 clocks/quarter → 48 clocks/sec → 20.833… ms
        let step = 20_833_333;
        for i in 0..12 {
            let (p, t) = clock_at(i as u64 * step);
            h.push(&p, t);
        }
        let bpm = h.clock.bpm_from_midi_clock().unwrap();
        assert!((bpm - 120.0).abs() < 0.5, "bpm={bpm}");
        assert!(!h.runaway());
    }

    #[test]
    fn runaway_fast_clock() {
        let mut h = ClockHealth::new();
        for i in 0..8 {
            h.push(&UmpMessage::midi1_system(0, 0xF8, 0, 0), i * 500_000);
        }
        assert!(h.runaway());
    }

    #[test]
    fn spp_and_start() {
        let mut h = ClockHealth::new();
        h.push(&UmpMessage::midi1_system(0, 0xFA, 0, 0), 0);
        assert_eq!(h.transport, Transport::Playing);
        h.push(&UmpMessage::midi1_system(0, 0xF2, 16, 0), 1);
        assert_eq!(h.song_pos, Some(16));
        assert!(h.spp_bars_4_4().unwrap().starts_with("2."));
        h.push(&UmpMessage::midi1_system(0, 0xFC, 0, 0), 2);
        assert_eq!(h.transport, Transport::Stopped);
    }

    #[test]
    fn mtc_assembles() {
        let mut m = MtcState::default();
        // 01:02:03:04 @ 25 fps — hours nibble type 7: rate=1 (25fps), hour msb=0
        let frames = 4u8;
        let secs = 3u8;
        let mins = 2u8;
        let hours = 1u8;
        m.push_qf(frames & 0x0F);
        m.push_qf(0x10 | ((frames >> 4) & 0x01));
        m.push_qf(0x20 | (secs & 0x0F));
        m.push_qf(0x30 | ((secs >> 4) & 0x03));
        m.push_qf(0x40 | (mins & 0x0F));
        m.push_qf(0x50 | ((mins >> 4) & 0x03));
        m.push_qf(0x60 | (hours & 0x0F));
        m.push_qf(0x70 | (1 << 1) | ((hours >> 4) & 0x01));
        assert_eq!(m.display().as_deref(), Some("01:02:03:04 @25"));
    }

    #[test]
    fn note_hist_counts_on() {
        let mut h = ClockHealth::new();
        h.push(&UmpMessage::midi1_channel_voice(0, 0x90, 60, 100), 1_000);
        h.push(&UmpMessage::midi1_channel_voice(0, 0x90, 61, 100), 11_000);
        assert_eq!(h.notes.len(), 1);
        assert_eq!(h.notes.mean_ns(), Some(10_000));
    }
}
