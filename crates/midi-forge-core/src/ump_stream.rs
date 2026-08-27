//! UMP Stream (type 0xF). Bit fields follow M2-104-UM §7.1 / linux `ump_msg.h`.
//!
//! Stream packets are four words and have no MIDI group: bits 27–26 of word0
//! are the form (complete / start / continue / end). Status is the 10-bit field
//! in bits 25–16.

use std::collections::HashMap;

use crate::decode::Decoded;
use crate::ump::UmpMessage;

pub const STREAM_FORM_COMPLETE: u8 = 0;
pub const STREAM_FORM_START: u8 = 1;
pub const STREAM_FORM_CONTINUE: u8 = 2;
pub const STREAM_FORM_END: u8 = 3;

pub const STREAM_STATUS_EP_DISCOVERY: u16 = 0x00;
pub const STREAM_STATUS_EP_INFO: u16 = 0x01;
pub const STREAM_STATUS_DEVICE_IDENTITY: u16 = 0x02;
pub const STREAM_STATUS_EP_NAME: u16 = 0x03;
pub const STREAM_STATUS_PRODUCT_ID: u16 = 0x04;
pub const STREAM_STATUS_STREAM_CFG_REQUEST: u16 = 0x05;
pub const STREAM_STATUS_STREAM_CFG: u16 = 0x06;
pub const STREAM_STATUS_FB_DISCOVERY: u16 = 0x10;
pub const STREAM_STATUS_FB_INFO: u16 = 0x11;
pub const STREAM_STATUS_FB_NAME: u16 = 0x12;

/// Endpoint Discovery filter: info, identity, name, product id, stream cfg.
pub const EP_FILTER_ALL: u8 = 0x1F;
/// Function Block Discovery filter: info + name.
pub const FB_FILTER_ALL: u8 = 0x03;
/// Function Block Discovery id meaning “every block”.
pub const FB_ID_ALL: u8 = 0xFF;

pub const UMP_VERSION_MAJOR: u8 = 1;
pub const UMP_VERSION_MINOR: u8 = 1;

pub const PROTOCOL_MIDI1: u8 = 1;
pub const PROTOCOL_MIDI2: u8 = 2;

pub const FB_DIR_INPUT: u8 = 1;
pub const FB_DIR_OUTPUT: u8 = 2;
pub const FB_DIR_BIDIRECTIONAL: u8 = 3;

const EP_NAME_BYTES: usize = 14;
const FB_NAME_BYTES: usize = 13;

/// Device Identity Notification payload (SysEx-style 7-bit family/model).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceIdentity {
    pub manufacturer: [u8; 3],
    pub family: u16,
    pub model: u16,
    pub software: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionBlock {
    pub id: u8,
    pub first_group: u8,
    pub n_groups: u8,
    pub midi1: bool,
    pub midi2: bool,
    pub name: String,
    pub direction: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointStream {
    pub name: String,
    pub product_id: String,
    pub identity: Option<DeviceIdentity>,
    pub midi1: bool,
    pub midi2: bool,
    pub jr_tx: bool,
    pub jr_rx: bool,
    pub protocol: u8,
    pub blocks: Vec<FunctionBlock>,
}

/// Assembles Endpoint / Function Block notifications from type 0xF packets.
#[derive(Clone, Debug, Default)]
pub struct StreamTracker {
    stream: EndpointStream,
    name_buf: Option<Vec<u8>>,
    product_buf: Option<Vec<u8>>,
    fb_name_buf: HashMap<u8, Vec<u8>>,
}

impl StreamTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> &EndpointStream {
        &self.stream
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn feed(&mut self, packet: &UmpMessage) {
        if packet.message_type() != 0xF {
            return;
        }
        let w0 = packet.words()[0];
        let w1 = packet.words().get(1).copied().unwrap_or(0);
        let form = stream_form(w0);
        match stream_status(w0) {
            STREAM_STATUS_EP_INFO => self.apply_ep_info(w1),
            STREAM_STATUS_DEVICE_IDENTITY => self.stream.identity = Some(parse_identity(packet)),
            STREAM_STATUS_EP_NAME => {
                if let Some(text) = assemble(
                    &mut self.name_buf,
                    form,
                    &text_bytes(packet, 0, EP_NAME_BYTES),
                ) {
                    self.stream.name = text;
                }
            }
            STREAM_STATUS_PRODUCT_ID => {
                if let Some(text) = assemble(
                    &mut self.product_buf,
                    form,
                    &text_bytes(packet, 0, EP_NAME_BYTES),
                ) {
                    self.stream.product_id = text;
                }
            }
            STREAM_STATUS_STREAM_CFG_REQUEST | STREAM_STATUS_STREAM_CFG => {
                apply_stream_cfg(&mut self.stream, w0);
            }
            STREAM_STATUS_FB_INFO => self.apply_fb_info(packet),
            STREAM_STATUS_FB_NAME => {
                let id = ((w0 >> 8) & 0x7F) as u8;
                let mut slot = self.fb_name_buf.remove(&id);
                if let Some(text) = assemble(&mut slot, form, &text_bytes(packet, 1, FB_NAME_BYTES))
                {
                    self.upsert_block(id).name = text;
                } else if let Some(rest) = slot {
                    self.fb_name_buf.insert(id, rest);
                }
            }
            _ => {}
        }
    }

    fn apply_ep_info(&mut self, w1: u32) {
        self.stream.midi1 = w1 & (1 << 8) != 0;
        self.stream.midi2 = w1 & (1 << 9) != 0;
        self.stream.jr_tx = w1 & (1 << 0) != 0;
        self.stream.jr_rx = w1 & (1 << 1) != 0;
        let proto = ((w1 >> 8) & 0xFF) as u8;
        if proto & PROTOCOL_MIDI2 != 0 {
            self.stream.protocol = PROTOCOL_MIDI2;
        } else if proto & PROTOCOL_MIDI1 != 0 {
            self.stream.protocol = PROTOCOL_MIDI1;
        }
    }

    fn apply_fb_info(&mut self, packet: &UmpMessage) {
        let parsed = parse_fb_info(packet);
        let block = self.upsert_block(parsed.id);
        block.first_group = parsed.first_group;
        block.n_groups = parsed.n_groups;
        block.midi1 = parsed.midi1;
        block.midi2 = parsed.midi2;
        block.direction = parsed.direction;
    }

    fn upsert_block(&mut self, id: u8) -> &mut FunctionBlock {
        if let Some(i) = self.stream.blocks.iter().position(|b| b.id == id) {
            return &mut self.stream.blocks[i];
        }
        self.stream.blocks.push(FunctionBlock {
            id,
            first_group: 0,
            n_groups: 0,
            midi1: false,
            midi2: false,
            name: String::new(),
            direction: 0,
        });
        self.stream.blocks.last_mut().expect("just pushed")
    }
}

/// Endpoint Discovery, UMP 1.1, filter 0x1F.
pub fn endpoint_discovery() -> UmpMessage {
    stream_words(
        pack_word0(
            STREAM_FORM_COMPLETE,
            STREAM_STATUS_EP_DISCOVERY,
            u16::from(UMP_VERSION_MAJOR) << 8 | u16::from(UMP_VERSION_MINOR),
        ),
        u32::from(EP_FILTER_ALL),
        0,
        0,
    )
}

/// Function Block Discovery for every block (id 0xFF), filter 0x03.
pub fn function_block_discovery() -> UmpMessage {
    stream_words(
        pack_word0(
            STREAM_FORM_COMPLETE,
            STREAM_STATUS_FB_DISCOVERY,
            u16::from(FB_ID_ALL) << 8 | u16::from(FB_FILTER_ALL),
        ),
        0,
        0,
        0,
    )
}

/// Stream Configuration Request, protocol = 2 (MIDI 2.0).
pub fn stream_configuration_request() -> UmpMessage {
    stream_words(
        pack_word0(
            STREAM_FORM_COMPLETE,
            STREAM_STATUS_STREAM_CFG_REQUEST,
            u16::from(PROTOCOL_MIDI2) << 8,
        ),
        0,
        0,
        0,
    )
}

/// The three inquiries sent when a UMP endpoint opens.
pub fn stream_inquiries() -> [UmpMessage; 3] {
    [
        endpoint_discovery(),
        function_block_discovery(),
        stream_configuration_request(),
    ]
}

pub fn fb_direction_label(direction: u8) -> &'static str {
    match direction {
        FB_DIR_INPUT => "in",
        FB_DIR_OUTPUT => "out",
        FB_DIR_BIDIRECTIONAL => "bidi",
        _ => "—",
    }
}

pub(crate) fn decode_stream(msg: &UmpMessage) -> Decoded {
    if msg.message_type() != 0xF {
        return other_stream(msg);
    }
    let w0 = msg.words()[0];
    let w1 = msg.words().get(1).copied().unwrap_or(0);
    let form = stream_form(w0);
    match stream_status(w0) {
        STREAM_STATUS_EP_DISCOVERY => Decoded::StreamEndpointDiscovery {
            filter: (w1 & 0xFF) as u8,
        },
        STREAM_STATUS_EP_INFO => Decoded::StreamEndpointInfo {
            midi1: w1 & (1 << 8) != 0,
            midi2: w1 & (1 << 9) != 0,
            jr_tx: w1 & (1 << 0) != 0,
            jr_rx: w1 & (1 << 1) != 0,
            n_function_blocks: ((w1 >> 24) & 0x7F) as u8,
        },
        STREAM_STATUS_DEVICE_IDENTITY => {
            let id = parse_identity(msg);
            Decoded::StreamDeviceIdentity {
                manufacturer: id.manufacturer,
                family: id.family,
                model: id.model,
            }
        }
        STREAM_STATUS_EP_NAME => Decoded::StreamEndpointName {
            form,
            text: utf8_chunk(&text_bytes(msg, 0, EP_NAME_BYTES)),
        },
        STREAM_STATUS_PRODUCT_ID => Decoded::StreamProductInstanceId {
            form,
            text: utf8_chunk(&text_bytes(msg, 0, EP_NAME_BYTES)),
        },
        STREAM_STATUS_STREAM_CFG_REQUEST => {
            let (protocol, jr_tx, jr_rx) = parse_stream_cfg(w0);
            Decoded::StreamConfigurationRequest {
                protocol,
                jr_tx,
                jr_rx,
            }
        }
        STREAM_STATUS_STREAM_CFG => {
            let (protocol, jr_tx, jr_rx) = parse_stream_cfg(w0);
            Decoded::StreamConfigurationNotification {
                protocol,
                jr_tx,
                jr_rx,
            }
        }
        STREAM_STATUS_FB_DISCOVERY => Decoded::StreamFunctionBlockDiscovery {
            id: ((w0 >> 8) & 0xFF) as u8,
            filter: (w0 & 0xFF) as u8,
        },
        STREAM_STATUS_FB_INFO => {
            let fb = parse_fb_info(msg);
            Decoded::StreamFunctionBlockInfo {
                id: fb.id,
                first_group: fb.first_group,
                n_groups: fb.n_groups,
                midi1: fb.midi1,
                midi2: fb.midi2,
                direction: fb.direction,
            }
        }
        STREAM_STATUS_FB_NAME => Decoded::StreamFunctionBlockName {
            id: ((w0 >> 8) & 0x7F) as u8,
            form,
            text: utf8_chunk(&text_bytes(msg, 1, FB_NAME_BYTES)),
        },
        _ => other_stream(msg),
    }
}

fn other_stream(msg: &UmpMessage) -> Decoded {
    Decoded::Other {
        message_type: 0xF,
        group: msg.group(),
        status: msg.status_byte(),
    }
}

fn stream_form(word0: u32) -> u8 {
    ((word0 >> 26) & 0x3) as u8
}

fn stream_status(word0: u32) -> u16 {
    ((word0 >> 16) & 0x3FF) as u16
}

fn pack_word0(form: u8, status: u16, data: u16) -> u32 {
    (0xF << 28)
        | (u32::from(form & 0x3) << 26)
        | (u32::from(status & 0x3FF) << 16)
        | u32::from(data)
}

fn stream_words(w0: u32, w1: u32, w2: u32, w3: u32) -> UmpMessage {
    UmpMessage::try_from_words(&[w0, w1, w2, w3]).expect("UMP type 0xF is four words")
}

fn parse_stream_cfg(w0: u32) -> (u8, bool, bool) {
    (((w0 >> 8) & 0xFF) as u8, w0 & 0x1 != 0, w0 & 0x2 != 0)
}

fn apply_stream_cfg(stream: &mut EndpointStream, w0: u32) {
    let protocol = ((w0 >> 8) & 0xFF) as u8;
    if protocol & PROTOCOL_MIDI2 != 0 {
        stream.protocol = PROTOCOL_MIDI2;
    } else if protocol & PROTOCOL_MIDI1 != 0 {
        stream.protocol = PROTOCOL_MIDI1;
    }
}

fn parse_identity(msg: &UmpMessage) -> DeviceIdentity {
    let w1 = msg.words().get(1).copied().unwrap_or(0);
    let w2 = msg.words().get(2).copied().unwrap_or(0);
    let w3 = msg.words().get(3).copied().unwrap_or(0);
    let family_lsb = ((w2 >> 24) & 0x7F) as u16;
    let family_msb = ((w2 >> 16) & 0x7F) as u16;
    let model_lsb = ((w2 >> 8) & 0x7F) as u16;
    let model_msb = (w2 & 0x7F) as u16;
    DeviceIdentity {
        manufacturer: [
            ((w1 >> 16) & 0x7F) as u8,
            ((w1 >> 8) & 0x7F) as u8,
            (w1 & 0x7F) as u8,
        ],
        family: (family_msb << 7) | family_lsb,
        model: (model_msb << 7) | model_lsb,
        software: w3.to_be_bytes(),
    }
}

struct ParsedFb {
    id: u8,
    first_group: u8,
    n_groups: u8,
    midi1: bool,
    midi2: bool,
    direction: u8,
}

fn parse_fb_info(msg: &UmpMessage) -> ParsedFb {
    let w0 = msg.words()[0];
    let w1 = msg.words().get(1).copied().unwrap_or(0);
    let midi10 = ((w0 >> 2) & 0x3) as u8;
    ParsedFb {
        id: ((w0 >> 8) & 0x7F) as u8,
        first_group: (w1 >> 24) as u8,
        n_groups: (w1 >> 16) as u8,
        midi1: midi10 == 1 || midi10 == 2,
        midi2: midi10 == 0,
        direction: (w0 & 0x3) as u8,
    }
}

fn text_bytes(msg: &UmpMessage, skip: usize, n: usize) -> Vec<u8> {
    let mut raw = Vec::with_capacity(14);
    let w0 = msg.words()[0];
    raw.push(((w0 >> 8) & 0xFF) as u8);
    raw.push((w0 & 0xFF) as u8);
    for word in msg.words().iter().skip(1) {
        raw.extend_from_slice(&word.to_be_bytes());
    }
    let end = (skip + n).min(raw.len());
    if skip >= end {
        Vec::new()
    } else {
        raw[skip..end].to_vec()
    }
}

fn utf8_chunk(bytes: &[u8]) -> String {
    String::from_utf8_lossy(payload_until_nul(bytes)).into_owned()
}

fn payload_until_nul(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|&b| b == 0) {
        Some(i) => &bytes[..i],
        None => bytes,
    }
}

fn assemble(buf: &mut Option<Vec<u8>>, form: u8, chunk: &[u8]) -> Option<String> {
    let piece = payload_until_nul(chunk);
    match form {
        STREAM_FORM_COMPLETE => {
            *buf = None;
            Some(String::from_utf8_lossy(piece).into_owned())
        }
        STREAM_FORM_START => {
            *buf = Some(piece.to_vec());
            None
        }
        STREAM_FORM_CONTINUE => {
            if let Some(b) = buf.as_mut() {
                b.extend_from_slice(piece);
            } else {
                *buf = Some(piece.to_vec());
            }
            None
        }
        STREAM_FORM_END => {
            let mut b = buf.take().unwrap_or_default();
            b.extend_from_slice(piece);
            Some(String::from_utf8_lossy(&b).into_owned())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{Decoded, decode};

    /// midi2 crate EndpointDiscovery builder: UMP 1.1, filter 0x1F.
    const EP_DISCOVERY: [u32; 4] = [0xF000_0101, 0x0000_001F, 0, 0];
    /// Endpoint Info: UMP 1.1, MIDI 2 capability, JR rx (not tx).
    const EP_INFO_M2_JRRX: [u32; 4] = [0xF001_0101, 0x0000_0202, 0, 0];
    /// midi2 crate FunctionBlockInfo builder fixture.
    const FB_INFO: [u32; 4] = [0xF011_9136, 0x0D08_0120, 0, 0];
    /// Function Block Discovery: all blocks, filter info+name.
    const FB_DISCOVERY: [u32; 4] = [0xF010_FF03, 0, 0, 0];
    /// Stream Configuration Request, protocol MIDI 2.
    const STREAM_CFG_REQ: [u32; 4] = [0xF005_0200, 0, 0, 0];
    /// Endpoint Name start: "Fo".
    const NAME_START: [u32; 4] = [0xF403_466F, 0, 0, 0];
    /// Endpoint Name end: "rge".
    const NAME_END: [u32; 4] = [0xFC03_7267, 0x6500_0000, 0, 0];

    fn pkt(words: [u32; 4]) -> UmpMessage {
        UmpMessage::try_from_words(&words).unwrap()
    }

    #[test]
    fn discovery_request_golden() {
        assert_eq!(endpoint_discovery().words(), &EP_DISCOVERY);
        match decode(&endpoint_discovery()) {
            Decoded::StreamEndpointDiscovery { filter } => assert_eq!(filter, 0x1F),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            decode(&endpoint_discovery()).kind_key(),
            "stream_ep_discovery"
        );
    }

    #[test]
    fn fb_discovery_and_stream_cfg_request_golden() {
        assert_eq!(function_block_discovery().words(), &FB_DISCOVERY);
        assert_eq!(stream_configuration_request().words(), &STREAM_CFG_REQ);
        match decode(&function_block_discovery()) {
            Decoded::StreamFunctionBlockDiscovery { id, filter } => {
                assert_eq!(id, 0xFF);
                assert_eq!(filter, 0x03);
            }
            other => panic!("{other:?}"),
        }
        match decode(&stream_configuration_request()) {
            Decoded::StreamConfigurationRequest {
                protocol,
                jr_tx,
                jr_rx,
            } => {
                assert_eq!(protocol, PROTOCOL_MIDI2);
                assert!(!jr_tx);
                assert!(!jr_rx);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            decode(&function_block_discovery()).kind_key(),
            "stream_fb_discovery"
        );
        assert_eq!(
            decode(&stream_configuration_request()).kind_key(),
            "stream_cfg_request"
        );
    }

    #[test]
    fn info_notify_midi2_jr_rx() {
        let m = pkt(EP_INFO_M2_JRRX);
        match decode(&m) {
            Decoded::StreamEndpointInfo {
                midi1,
                midi2,
                jr_tx,
                jr_rx,
                n_function_blocks,
            } => {
                assert!(!midi1);
                assert!(midi2);
                assert!(!jr_tx);
                assert!(jr_rx);
                assert_eq!(n_function_blocks, 0);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(decode(&m).kind_key(), "stream_ep_info");
        let mut t = StreamTracker::new();
        t.feed(&m);
        let s = t.snapshot();
        assert!(s.midi2);
        assert!(!s.midi1);
        assert!(s.jr_rx);
        assert!(!s.jr_tx);
        assert_eq!(s.protocol, PROTOCOL_MIDI2);
    }

    #[test]
    fn function_block_info_golden() {
        let m = pkt(FB_INFO);
        match decode(&m) {
            Decoded::StreamFunctionBlockInfo {
                id,
                first_group,
                n_groups,
                midi1,
                midi2,
                direction,
            } => {
                assert_eq!(id, 0x11);
                assert_eq!(first_group, 0x0D);
                assert_eq!(n_groups, 0x08);
                assert!(midi1);
                assert!(!midi2);
                assert_eq!(direction, FB_DIR_OUTPUT);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(decode(&m).kind_key(), "stream_fb_info");
        let mut t = StreamTracker::new();
        t.feed(&m);
        let b = &t.snapshot().blocks[0];
        assert_eq!(b.id, 0x11);
        assert_eq!(b.first_group, 0x0D);
        assert_eq!(b.n_groups, 8);
        assert!(b.midi1);
        assert!(!b.midi2);
        assert_eq!(b.direction, FB_DIR_OUTPUT);
    }

    #[test]
    fn chunked_endpoint_name_forge() {
        let start = pkt(NAME_START);
        let end = pkt(NAME_END);
        match decode(&start) {
            Decoded::StreamEndpointName { form, text } => {
                assert_eq!(form, STREAM_FORM_START);
                assert_eq!(text, "Fo");
            }
            other => panic!("{other:?}"),
        }
        match decode(&end) {
            Decoded::StreamEndpointName { form, text } => {
                assert_eq!(form, STREAM_FORM_END);
                assert_eq!(text, "rge");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(decode(&start).kind_key(), "stream_ep_name");
        let mut t = StreamTracker::new();
        t.feed(&start);
        assert_eq!(t.snapshot().name, "");
        t.feed(&end);
        assert_eq!(t.snapshot().name, "Forge");
    }

    #[test]
    fn complete_endpoint_name_forge() {
        let m = pkt([0xF003_466F, 0x7267_6500, 0, 0]);
        let mut t = StreamTracker::new();
        t.feed(&m);
        assert_eq!(t.snapshot().name, "Forge");
    }

    #[test]
    fn stream_inquiries_are_the_three_constructors() {
        let q = stream_inquiries();
        assert_eq!(q[0], endpoint_discovery());
        assert_eq!(q[1], function_block_discovery());
        assert_eq!(q[2], stream_configuration_request());
    }

    #[test]
    fn device_identity_midi2_crate_fixture() {
        let m = pkt([0xF002_0000, 0x000F_3328, 0x4A1E_1870, 0x4354_3201]);
        match decode(&m) {
            Decoded::StreamDeviceIdentity {
                manufacturer,
                family,
                model,
            } => {
                assert_eq!(manufacturer, [0x0F, 0x33, 0x28]);
                assert_eq!(family, 0xF4A);
                assert_eq!(model, 0x3818);
            }
            other => panic!("{other:?}"),
        }
        let mut t = StreamTracker::new();
        t.feed(&m);
        let id = t.snapshot().identity.as_ref().unwrap();
        assert_eq!(id.manufacturer, [0x0F, 0x33, 0x28]);
        assert_eq!(id.software, [0x43, 0x54, 0x32, 0x01]);
    }
}
