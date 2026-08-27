//! UMP Flex Data (type 0xD). Bit fields follow M2-104-UM §7.5 / Table 32.

use std::collections::HashMap;

use crate::decode::Decoded;
use crate::sysex::SysexError;
use crate::ump::UmpMessage;

/// Form (bits 23–22 of word0): complete / start / continue / end.
pub const FLEX_FORM_COMPLETE: u8 = 0;
pub const FLEX_FORM_START: u8 = 1;
pub const FLEX_FORM_CONTINUE: u8 = 2;
pub const FLEX_FORM_END: u8 = 3;

/// Address (bits 21–20 of word0): 1 = Group (setup messages).
pub const FLEX_ADDR_GROUP: u8 = 1;

pub const FLEX_BANK_SETUP: u8 = 0;
pub const FLEX_BANK_METADATA: u8 = 1;
pub const FLEX_BANK_PERF_TEXT: u8 = 2;

pub const FLEX_STATUS_SET_TEMPO: u8 = 0x00;
pub const FLEX_STATUS_SET_TIME_SIG: u8 = 0x01;
pub const FLEX_STATUS_SET_METRONOME: u8 = 0x02;
pub const FLEX_STATUS_SET_KEY_SIG: u8 = 0x05;

const TEXT_BYTES_PER_PACKET: usize = 12;

/// 1 µs = 100 × 10 ns (M2-104-UM §7.5.3 wire unit).
const TEN_NS_PER_US: u32 = 100;
/// 60 s in 10 ns units; BPM = this / ten_ns_per_quarter.
const TEN_NS_PER_MINUTE: f64 = 6_000_000_000.0;

/// Status Bank + Status for Flex Data text messages (Table 16).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlexTextKind {
    UnknownMetadata,
    ProjectName,
    CompositionName,
    MidiClipName,
    CopyrightNotice,
    ComposerName,
    LyricistName,
    ArrangerName,
    PublisherName,
    PrimaryPerformer,
    AccompanyingPerformer,
    RecordingDate,
    RecordingLocation,
    UnknownPerformance,
    Lyric,
    LyricLanguage,
    Ruby,
    RubyLanguage,
}

impl FlexTextKind {
    pub fn from_bank_status(bank: u8, status: u8) -> Option<Self> {
        Some(match (bank, status) {
            (FLEX_BANK_METADATA, 0x00) => Self::UnknownMetadata,
            (FLEX_BANK_METADATA, 0x01) => Self::ProjectName,
            (FLEX_BANK_METADATA, 0x02) => Self::CompositionName,
            (FLEX_BANK_METADATA, 0x03) => Self::MidiClipName,
            (FLEX_BANK_METADATA, 0x04) => Self::CopyrightNotice,
            (FLEX_BANK_METADATA, 0x05) => Self::ComposerName,
            (FLEX_BANK_METADATA, 0x06) => Self::LyricistName,
            (FLEX_BANK_METADATA, 0x07) => Self::ArrangerName,
            (FLEX_BANK_METADATA, 0x08) => Self::PublisherName,
            (FLEX_BANK_METADATA, 0x09) => Self::PrimaryPerformer,
            (FLEX_BANK_METADATA, 0x0A) => Self::AccompanyingPerformer,
            (FLEX_BANK_METADATA, 0x0B) => Self::RecordingDate,
            (FLEX_BANK_METADATA, 0x0C) => Self::RecordingLocation,
            (FLEX_BANK_METADATA, _) => Self::UnknownMetadata,
            (FLEX_BANK_PERF_TEXT, 0x00) => Self::UnknownPerformance,
            (FLEX_BANK_PERF_TEXT, 0x01) => Self::Lyric,
            (FLEX_BANK_PERF_TEXT, 0x02) => Self::LyricLanguage,
            (FLEX_BANK_PERF_TEXT, 0x03) => Self::Ruby,
            (FLEX_BANK_PERF_TEXT, 0x04) => Self::RubyLanguage,
            (FLEX_BANK_PERF_TEXT, _) => Self::UnknownPerformance,
            _ => return None,
        })
    }

    pub fn bank_status(self) -> (u8, u8) {
        match self {
            Self::UnknownMetadata => (FLEX_BANK_METADATA, 0x00),
            Self::ProjectName => (FLEX_BANK_METADATA, 0x01),
            Self::CompositionName => (FLEX_BANK_METADATA, 0x02),
            Self::MidiClipName => (FLEX_BANK_METADATA, 0x03),
            Self::CopyrightNotice => (FLEX_BANK_METADATA, 0x04),
            Self::ComposerName => (FLEX_BANK_METADATA, 0x05),
            Self::LyricistName => (FLEX_BANK_METADATA, 0x06),
            Self::ArrangerName => (FLEX_BANK_METADATA, 0x07),
            Self::PublisherName => (FLEX_BANK_METADATA, 0x08),
            Self::PrimaryPerformer => (FLEX_BANK_METADATA, 0x09),
            Self::AccompanyingPerformer => (FLEX_BANK_METADATA, 0x0A),
            Self::RecordingDate => (FLEX_BANK_METADATA, 0x0B),
            Self::RecordingLocation => (FLEX_BANK_METADATA, 0x0C),
            Self::UnknownPerformance => (FLEX_BANK_PERF_TEXT, 0x00),
            Self::Lyric => (FLEX_BANK_PERF_TEXT, 0x01),
            Self::LyricLanguage => (FLEX_BANK_PERF_TEXT, 0x02),
            Self::Ruby => (FLEX_BANK_PERF_TEXT, 0x03),
            Self::RubyLanguage => (FLEX_BANK_PERF_TEXT, 0x04),
        }
    }
}

/// Assembled Flex Data text (complete packet or start..end).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlexText {
    pub group: u8,
    pub kind: FlexTextKind,
    pub text: String,
}

/// Concatenate chunked Flex Data UTF-8 (form start/continue/end).
#[derive(Default)]
pub struct FlexTextAssembler {
    streams: HashMap<(u8, u8, u8), Vec<u8>>,
}

impl FlexTextAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.streams.clear();
    }

    /// Feed one UMP. `Ok(Some(text))` when a message finishes; `Ok(None)` while
    /// assembling or if the packet is not Flex Data text. Unknown CONTINUE/END
    /// error as `SysexError::Framing` for that stream only.
    pub fn push(&mut self, packet: &UmpMessage) -> Result<Option<FlexText>, SysexError> {
        let Some(hdr) = flex_header(packet) else {
            return Ok(None);
        };
        let Some(kind) = FlexTextKind::from_bank_status(hdr.bank, hdr.status) else {
            return Ok(None);
        };
        let key = (hdr.group, hdr.bank, hdr.status);
        let payload = flex_payload12(packet);
        let chunk = payload_until_nul(&payload);
        match hdr.form {
            FLEX_FORM_COMPLETE => {
                self.streams.remove(&key);
                Ok(Some(FlexText {
                    group: hdr.group,
                    kind,
                    text: String::from_utf8_lossy(chunk).into_owned(),
                }))
            }
            FLEX_FORM_START => {
                self.streams.insert(key, chunk.to_vec());
                Ok(None)
            }
            FLEX_FORM_CONTINUE => {
                let Some(buf) = self.streams.get_mut(&key) else {
                    return Err(SysexError::Framing);
                };
                buf.extend_from_slice(chunk);
                Ok(None)
            }
            FLEX_FORM_END => {
                let Some(mut buf) = self.streams.remove(&key) else {
                    return Err(SysexError::Framing);
                };
                buf.extend_from_slice(chunk);
                Ok(Some(FlexText {
                    group: hdr.group,
                    kind,
                    text: String::from_utf8_lossy(&buf).into_owned(),
                }))
            }
            _ => Ok(None),
        }
    }
}

/// BPM from the raw 10-nanosecond-per-quarter field. `None` if the field is 0.
pub fn flex_tempo_bpm(ten_ns_per_quarter: u32) -> Option<f64> {
    if ten_ns_per_quarter == 0 {
        return None;
    }
    Some(TEN_NS_PER_MINUTE / f64::from(ten_ns_per_quarter))
}

/// Set Tempo. `microseconds_per_quarter` is SMF-style µs/qn; the wire field is
/// 10-nanosecond units (`µs * 100`) per M2-104-UM §7.5.3.
pub fn flex_set_tempo(group: u8, microseconds_per_quarter: u32) -> UmpMessage {
    let ten_ns = microseconds_per_quarter.saturating_mul(TEN_NS_PER_US);
    flex_words(
        flex_word0(
            group,
            FLEX_FORM_COMPLETE,
            FLEX_ADDR_GROUP,
            0,
            FLEX_BANK_SETUP,
            FLEX_STATUS_SET_TEMPO,
        ),
        ten_ns,
        0,
        0,
    )
}

/// Set Time Signature. `denominator` is the negative power of 2 (2 = quarter).
pub fn flex_set_time_sig(
    group: u8,
    numerator: u8,
    denominator: u8,
    number_of_32nd_notes: u8,
) -> UmpMessage {
    let word1 = (u32::from(numerator) << 24)
        | (u32::from(denominator) << 16)
        | (u32::from(number_of_32nd_notes) << 8);
    flex_words(
        flex_word0(
            group,
            FLEX_FORM_COMPLETE,
            FLEX_ADDR_GROUP,
            0,
            FLEX_BANK_SETUP,
            FLEX_STATUS_SET_TIME_SIG,
        ),
        word1,
        0,
        0,
    )
}

/// Set Metronome (M2-104-UM §7.5.5).
pub fn flex_set_metronome(
    group: u8,
    clocks_per_primary: u8,
    bar_accent1: u8,
    bar_accent2: u8,
    bar_accent3: u8,
    subdivision_clicks1: u8,
    subdivision_clicks2: u8,
) -> UmpMessage {
    let word1 = (u32::from(clocks_per_primary) << 24)
        | (u32::from(bar_accent1) << 16)
        | (u32::from(bar_accent2) << 8)
        | u32::from(bar_accent3);
    let word2 = (u32::from(subdivision_clicks1) << 24) | (u32::from(subdivision_clicks2) << 16);
    flex_words(
        flex_word0(
            group,
            FLEX_FORM_COMPLETE,
            FLEX_ADDR_GROUP,
            0,
            FLEX_BANK_SETUP,
            FLEX_STATUS_SET_METRONOME,
        ),
        word1,
        word2,
        0,
    )
}

/// Set Key Signature. `sharps_flats` is signed (−7..=7; 8 = non-standard).
/// `tonic` is the 4-bit tonic nibble (0 non-standard, 1=A … 7=G).
pub fn flex_set_key_sig(group: u8, sharps_flats: i8, tonic: u8) -> UmpMessage {
    let sf = encode_sharps_flats(sharps_flats);
    let word1 = (u32::from(sf) << 28) | (u32::from(tonic & 0x0F) << 24);
    flex_words(
        flex_word0(
            group,
            FLEX_FORM_COMPLETE,
            FLEX_ADDR_GROUP,
            0,
            FLEX_BANK_SETUP,
            FLEX_STATUS_SET_KEY_SIG,
        ),
        word1,
        0,
        0,
    )
}

/// Lyric (performance text, bank 2 status 1). Splits at 12 UTF-8 bytes.
pub fn flex_lyric(group: u8, text: &str) -> Vec<UmpMessage> {
    flex_text(group, FlexTextKind::Lyric, text)
}

/// Flex Data text packets for any Table 16 kind.
pub fn flex_text(group: u8, kind: FlexTextKind, text: &str) -> Vec<UmpMessage> {
    let (bank, status) = kind.bank_status();
    let bytes = text.as_bytes();
    if bytes.len() <= TEXT_BYTES_PER_PACKET {
        return vec![flex_text_packet(
            group,
            FLEX_FORM_COMPLETE,
            bank,
            status,
            bytes,
        )];
    }
    let mut out = Vec::new();
    let mut offset = 0;
    let mut first = true;
    while offset < bytes.len() {
        let take = (bytes.len() - offset).min(TEXT_BYTES_PER_PACKET);
        let last = offset + take == bytes.len();
        let form = match (first, last) {
            (true, true) => FLEX_FORM_COMPLETE,
            (true, false) => FLEX_FORM_START,
            (false, false) => FLEX_FORM_CONTINUE,
            (false, true) => FLEX_FORM_END,
        };
        out.push(flex_text_packet(
            group,
            form,
            bank,
            status,
            &bytes[offset..offset + take],
        ));
        offset += take;
        first = false;
    }
    out
}

pub(crate) fn decode_flex(msg: &UmpMessage) -> Decoded {
    let Some(hdr) = flex_header(msg) else {
        return other_flex(msg);
    };
    let words = msg.words();
    let w1 = words.get(1).copied().unwrap_or(0);
    let w2 = words.get(2).copied().unwrap_or(0);
    match (hdr.bank, hdr.status) {
        (FLEX_BANK_SETUP, FLEX_STATUS_SET_TEMPO) => Decoded::FlexTempo {
            group: hdr.group,
            ten_ns_per_quarter: w1,
        },
        (FLEX_BANK_SETUP, FLEX_STATUS_SET_TIME_SIG) => Decoded::FlexTimeSig {
            group: hdr.group,
            numerator: (w1 >> 24) as u8,
            denominator: (w1 >> 16) as u8,
            number_of_32nd_notes: (w1 >> 8) as u8,
        },
        (FLEX_BANK_SETUP, FLEX_STATUS_SET_METRONOME) => Decoded::FlexMetronome {
            group: hdr.group,
            clocks_per_primary: (w1 >> 24) as u8,
            bar_accent1: (w1 >> 16) as u8,
            bar_accent2: (w1 >> 8) as u8,
            bar_accent3: w1 as u8,
            subdivision_clicks1: (w2 >> 24) as u8,
            subdivision_clicks2: (w2 >> 16) as u8,
        },
        (FLEX_BANK_SETUP, FLEX_STATUS_SET_KEY_SIG) => Decoded::FlexKeySig {
            group: hdr.group,
            sharps_flats: decode_sharps_flats((w1 >> 28) as u8),
            tonic: ((w1 >> 24) & 0x0F) as u8,
        },
        (bank, status) => match FlexTextKind::from_bank_status(bank, status) {
            Some(kind) => {
                let payload = flex_payload12(msg);
                let chunk = payload_until_nul(&payload);
                Decoded::FlexText {
                    group: hdr.group,
                    kind,
                    text: String::from_utf8_lossy(chunk).into_owned(),
                }
            }
            None => other_flex(msg),
        },
    }
}

struct FlexHeader {
    group: u8,
    form: u8,
    bank: u8,
    status: u8,
}

fn flex_header(msg: &UmpMessage) -> Option<FlexHeader> {
    if msg.message_type() != 0xD {
        return None;
    }
    let w0 = msg.words()[0];
    Some(FlexHeader {
        group: ((w0 >> 24) & 0xF) as u8,
        form: ((w0 >> 22) & 0x3) as u8,
        bank: ((w0 >> 8) & 0xFF) as u8,
        status: (w0 & 0xFF) as u8,
    })
}

fn other_flex(msg: &UmpMessage) -> Decoded {
    Decoded::Other {
        message_type: 0xD,
        group: msg.group(),
        status: msg.status_byte(),
    }
}

fn flex_words(word0: u32, word1: u32, word2: u32, word3: u32) -> UmpMessage {
    UmpMessage::try_from_words(&[word0, word1, word2, word3]).expect("UMP type 0xD is four words")
}

fn flex_word0(group: u8, form: u8, addr: u8, channel: u8, bank: u8, status: u8) -> u32 {
    (0xD << 28)
        | (u32::from(group & 0x0F) << 24)
        | (u32::from(form & 0x3) << 22)
        | (u32::from(addr & 0x3) << 20)
        | (u32::from(channel & 0x0F) << 16)
        | (u32::from(bank) << 8)
        | u32::from(status)
}

fn flex_text_packet(group: u8, form: u8, bank: u8, status: u8, data: &[u8]) -> UmpMessage {
    let mut payload = [0u8; TEXT_BYTES_PER_PACKET];
    let n = data.len().min(TEXT_BYTES_PER_PACKET);
    payload[..n].copy_from_slice(&data[..n]);
    let word1 = u32::from_be_bytes(payload[0..4].try_into().unwrap());
    let word2 = u32::from_be_bytes(payload[4..8].try_into().unwrap());
    let word3 = u32::from_be_bytes(payload[8..12].try_into().unwrap());
    flex_words(
        flex_word0(group, form, FLEX_ADDR_GROUP, 0, bank, status),
        word1,
        word2,
        word3,
    )
}

fn flex_payload12(msg: &UmpMessage) -> [u8; 12] {
    let w1 = msg.words().get(1).copied().unwrap_or(0);
    let w2 = msg.words().get(2).copied().unwrap_or(0);
    let w3 = msg.words().get(3).copied().unwrap_or(0);
    let mut out = [0u8; 12];
    out[0..4].copy_from_slice(&w1.to_be_bytes());
    out[4..8].copy_from_slice(&w2.to_be_bytes());
    out[8..12].copy_from_slice(&w3.to_be_bytes());
    out
}

fn payload_until_nul(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|&b| b == 0) {
        Some(i) => &bytes[..i],
        None => bytes,
    }
}

fn encode_sharps_flats(sf: i8) -> u8 {
    if sf == 8 { 0x8 } else { (sf as u8) & 0x0F }
}

fn decode_sharps_flats(nibble: u8) -> i8 {
    let n = nibble & 0x0F;
    if n == 0x8 {
        8
    } else if n & 0x8 != 0 {
        (n as i8) | !0x0F
    } else {
        n as i8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{Decoded, decode};

    #[test]
    fn midi2_crate_tempo_fixture() {
        let m = UmpMessage::try_from_words(&[0xD710_0000, 0xF751_FE05, 0, 0]).unwrap();
        match decode(&m) {
            Decoded::FlexTempo {
                group,
                ten_ns_per_quarter,
            } => {
                assert_eq!(group, 7);
                assert_eq!(ten_ns_per_quarter, 0xF751_FE05);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn key_sig_five_flats() {
        // midi2 crate: 5 flats, tonic D → word1 0xB400_0000 (4-bit two's complement).
        let m = UmpMessage::try_from_words(&[0xD410_0005, 0xB400_0000, 0, 0]).unwrap();
        match decode(&m) {
            Decoded::FlexKeySig {
                sharps_flats,
                tonic,
                ..
            } => {
                assert_eq!(sharps_flats, -5);
                assert_eq!(tonic, 0x4);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            flex_set_key_sig(4, -5, 0x4).words(),
            &[0xD410_0005, 0xB400_0000, 0, 0]
        );
    }

    #[test]
    fn assembler_complete_lyric() {
        let pkts = flex_lyric(2, "Hi");
        let mut asm = FlexTextAssembler::new();
        let done = asm.push(&pkts[0]).unwrap().expect("complete");
        assert_eq!(done.group, 2);
        assert_eq!(done.text, "Hi");
    }

    #[test]
    fn assembler_unknown_end_is_framing() {
        let end = flex_text_packet(0, FLEX_FORM_END, FLEX_BANK_PERF_TEXT, 0x01, b"x");
        let mut asm = FlexTextAssembler::new();
        assert_eq!(asm.push(&end), Err(SysexError::Framing));
    }
}
