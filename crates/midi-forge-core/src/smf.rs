//! Standard MIDI File format 0 (single track) record/play helpers.

use crate::event::{MidiEvent, Timestamp};
use crate::midi1::packed_short_from_ump;
use crate::midi2::downscale_to_midi1;
use crate::ump::UmpMessage;

const TPQN: u16 = 480;
const TEMPO_US: u32 = 500_000; // 120 BPM

pub fn write_smf0(events: &[MidiEvent]) -> Vec<u8> {
    let mut track = Vec::new();
    let mut last_tick: u32 = 0;
    let mut t0: Option<u64> = None;
    write_vlq(&mut track, 0);
    track.extend_from_slice(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]); // tempo 500000
    for ev in events {
        for packet in downscale_to_midi1(&ev.packet) {
            let Some(packed) = packed_short_from_ump(&packet) else {
                continue;
            };
            let t = ev.time.nanos;
            let origin = *t0.get_or_insert(t);
            let tick = nanos_to_tick(t.saturating_sub(origin));
            let delta = tick.saturating_sub(last_tick);
            last_tick = tick;
            write_vlq(&mut track, delta);
            let status = (packed & 0xFF) as u8;
            let d1 = ((packed >> 8) & 0xFF) as u8;
            let d2 = ((packed >> 16) & 0xFF) as u8;
            match crate::midi1::midi1_data_len(status) {
                0 => track.push(status),
                1 => track.extend_from_slice(&[status, d1]),
                _ => track.extend_from_slice(&[status, d1, d2]),
            }
        }
    }
    write_vlq(&mut track, 0);
    track.extend_from_slice(&[0xFF, 0x2F, 0x00]);

    let mut out = Vec::from(b"MThd\x00\x00\x00\x06\x00\x00\x00\x01");
    out.extend_from_slice(&TPQN.to_be_bytes());
    out.extend_from_slice(b"MTrk");
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend(track);
    out
}

pub fn nanos_to_tick(ns: u64) -> u32 {
    let ticks = ns as u128 * u128::from(TPQN) / (u128::from(TEMPO_US) * 1_000);
    ticks.min(u128::from(u32::MAX)) as u32
}

pub fn tick_to_nanos(tick: u32) -> u64 {
    (u128::from(tick) * u128::from(TEMPO_US) * 1_000 / u128::from(TPQN)) as u64
}

fn write_vlq(out: &mut Vec<u8>, mut v: u32) {
    if v == 0 {
        out.push(0);
        return;
    }
    let mut buf = [0u8; 5];
    let mut n = 0;
    buf[n] = (v & 0x7F) as u8;
    n += 1;
    v >>= 7;
    while v > 0 {
        buf[n] = (v & 0x7F) as u8 | 0x80;
        n += 1;
        v >>= 7;
    }
    for i in (0..n).rev() {
        out.push(buf[i]);
    }
}

/// Play list: host timestamps from tick 0.
pub fn events_from_smf0(bytes: &[u8]) -> Result<Vec<MidiEvent>, String> {
    if bytes.len() < 22 || &bytes[0..4] != b"MThd" {
        return Err("not an SMF".into());
    }
    let mut i = 8 + 6; // skip header chunk payload (format/ntrks/div already 6)
    // find MTrk
    while i + 8 <= bytes.len() && &bytes[i..i + 4] != b"MTrk" {
        i += 1;
    }
    if i + 8 > bytes.len() {
        return Err("no MTrk".into());
    }
    let len = u32::from_be_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
    let track = &bytes[i + 8..bytes.len().min(i + 8 + len)];
    let mut pos = 0;
    let mut tick: u32 = 0;
    let mut running = 0u8;
    let mut out = Vec::new();
    while pos < track.len() {
        let (delta, n) = read_vlq(&track[pos..])?;
        pos += n;
        tick = tick.saturating_add(delta);
        if pos >= track.len() {
            break;
        }
        let mut status = track[pos];
        if status < 0x80 {
            status = running;
        } else {
            pos += 1;
            if status < 0xF0 {
                running = status;
            }
        }
        if status == 0xFF {
            if pos + 1 >= track.len() {
                break;
            }
            let meta = track[pos];
            pos += 1;
            let (mlen, n) = read_vlq(&track[pos..])?;
            pos += n + mlen as usize;
            if meta == 0x2F {
                break;
            }
            continue;
        }
        let need = crate::midi1::midi1_data_len(status) as usize;
        if pos + need > track.len() {
            break;
        }
        let d1 = if need > 0 { track[pos] } else { 0 };
        let d2 = if need > 1 { track[pos + 1] } else { 0 };
        pos += need;
        let packet = if status >= 0xF0 {
            UmpMessage::midi1_system(0, status, d1, d2)
        } else {
            UmpMessage::midi1_channel_voice(0, status, d1, d2)
        };
        out.push(MidiEvent::new(
            Timestamp::from_nanos(tick_to_nanos(tick)),
            crate::event::PortId(0),
            packet,
        ));
    }
    Ok(out)
}

fn read_vlq(bytes: &[u8]) -> Result<(u32, usize), String> {
    let mut v = 0u32;
    for (i, b) in bytes.iter().take(4).enumerate() {
        v = (v << 7) | u32::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Ok((v, i + 1));
        }
    }
    Err("bad VLQ".into())
}

#[derive(Clone, Debug, Default)]
pub struct SessionRecorder {
    pub recording: bool,
    events: Vec<MidiEvent>,
}

impl SessionRecorder {
    pub fn push(&mut self, event: MidiEvent) {
        if self.recording {
            self.events.push(event);
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn to_smf(&self) -> Vec<u8> {
        write_smf0(&self.events)
    }

    pub fn events(&self) -> &[MidiEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::PortId;

    #[test]
    fn roundtrip_note() {
        let ev = MidiEvent::new(
            Timestamp::from_nanos(0),
            PortId(0),
            UmpMessage::midi1_channel_voice(0, 0x90, 60, 100),
        );
        let bytes = write_smf0(&[ev]);
        assert!(bytes.starts_with(b"MThd"));
        let back = events_from_smf0(&bytes).unwrap();
        assert!(!back.is_empty());
        assert_eq!(back[0].packet.status_byte(), 0x90);
        assert_eq!(back[0].packet.data1(), 60);
    }
}
