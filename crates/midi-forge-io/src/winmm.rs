//! WinMM enumeration, capture, and short-message output.

use std::collections::HashMap;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use midi_forge_core::{
    Midi1Parser, MidiEvent, PortId, Timestamp, UmpMessage, packed_short_from_ump,
    ump_from_packed_short,
};

use crate::backend::{Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint};
use crate::error::IoError;

const MAXPNAMELEN: usize = 32;
const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_ALLOCATED: u32 = 4;
const CALLBACK_NULL: u32 = 0;
const CALLBACK_FUNCTION: u32 = 0x0003_0000;
const MIM_DATA: u32 = 0x3C3;
const MIM_LONGDATA: u32 = 0x3C4;
const SYSEX_BUFFERS: usize = 8;
const SYSEX_CAP: usize = 1024;
const QUEUE_CAP: usize = 4096;

#[repr(C)]
struct MidiInCapsW {
    _w_mid: u16,
    _w_pid: u16,
    _driver_version: u32,
    name: [u16; MAXPNAMELEN],
    _support: u32,
}

#[repr(C)]
struct MidiOutCapsW {
    _w_mid: u16,
    _w_pid: u16,
    _driver_version: u32,
    name: [u16; MAXPNAMELEN],
    _technology: u16,
    _voices: u16,
    _notes: u16,
    _channel_mask: u16,
    _support: u32,
}

/// 64-bit Windows `MIDIHDR` (120 bytes).
#[repr(C)]
struct MidiHdr {
    lp_data: *mut u8,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    _pad0: u32,
    next: *mut MidiHdr,
    reserved: usize,
    offset: u32,
    _pad1: u32,
    reserved2: [usize; 8],
}

impl MidiHdr {
    fn empty() -> Self {
        Self {
            lp_data: ptr::null_mut(),
            buffer_length: 0,
            bytes_recorded: 0,
            user: 0,
            flags: 0,
            _pad0: 0,
            next: ptr::null_mut(),
            reserved: 0,
            offset: 0,
            _pad1: 0,
            reserved2: [0; 8],
        }
    }
}

#[link(name = "winmm")]
unsafe extern "system" {
    fn midiInGetNumDevs() -> u32;
    fn midiInGetDevCapsW(device_id: usize, caps: *mut MidiInCapsW, size: u32) -> u32;
    fn midiOutGetNumDevs() -> u32;
    fn midiOutGetDevCapsW(device_id: usize, caps: *mut MidiOutCapsW, size: u32) -> u32;
    fn midiInOpen(
        handle: *mut usize,
        device_id: usize,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn midiInStart(handle: usize) -> u32;
    fn midiInStop(handle: usize) -> u32;
    fn midiInReset(handle: usize) -> u32;
    fn midiInClose(handle: usize) -> u32;
    fn midiInPrepareHeader(handle: usize, header: *mut MidiHdr, size: u32) -> u32;
    fn midiInUnprepareHeader(handle: usize, header: *mut MidiHdr, size: u32) -> u32;
    fn midiInAddBuffer(handle: usize, header: *mut MidiHdr, size: u32) -> u32;
    fn midiOutOpen(
        handle: *mut usize,
        device_id: usize,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> u32;
    fn midiOutClose(handle: usize) -> u32;
    fn midiOutShortMsg(handle: usize, msg: u32) -> u32;
}

#[derive(Clone, Copy)]
struct CaptureFrame {
    time_ms: u32,
    port: PortId,
    kind: FrameKind,
}

#[derive(Clone, Copy)]
#[allow(clippy::large_enum_variant)] // Copy SysEx bytes; WinMM callback must not allocate.
enum FrameKind {
    Short(u32),
    SysEx { len: u16, buf: [u8; SYSEX_CAP] },
}

struct InputShared {
    port: PortId,
    tx: SyncSender<CaptureFrame>,
    dropped: Arc<AtomicU64>,
    live: AtomicBool,
}

struct PreparedSysex {
    hdr: Box<MidiHdr>,
    _data: Box<[u8; SYSEX_CAP]>,
}

struct OpenInput {
    handle: usize,
    shared: Arc<InputShared>,
    _buffers: Vec<PreparedSysex>,
}

struct OpenOutput {
    handle: usize,
}

pub struct WinMmBackend {
    endpoints: Vec<Endpoint>,
    inputs: HashMap<String, OpenInput>,
    outputs: HashMap<String, OpenOutput>,
    tx: SyncSender<CaptureFrame>,
    rx: Receiver<CaptureFrame>,
    dropped: Arc<AtomicU64>,
    parsers: HashMap<PortId, Midi1Parser>,
}

impl WinMmBackend {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(QUEUE_CAP);
        let mut this = Self {
            endpoints: Vec::new(),
            inputs: HashMap::new(),
            outputs: HashMap::new(),
            tx,
            rx,
            dropped: Arc::new(AtomicU64::new(0)),
            parsers: HashMap::new(),
        };
        let _ = this.refresh();
        this
    }
}

impl Default for WinMmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WinMmBackend {
    fn drop(&mut self) {
        let inputs: Vec<String> = self.inputs.keys().cloned().collect();
        for id in inputs {
            let _ = self.close_input(&EndpointId(id));
        }
        let outputs: Vec<String> = self.outputs.keys().cloned().collect();
        for id in outputs {
            let _ = self.close_output(&EndpointId(id));
        }
    }
}

impl MidiBackend for WinMmBackend {
    fn name(&self) -> &'static str {
        "winmm"
    }

    fn refresh(&mut self) -> Result<(), IoError> {
        self.endpoints = enumerate()?;
        Ok(())
    }

    fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    fn open_input(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError> {
        if self.inputs.contains_key(&id.0) {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        let device_id = parse_index(&id.0, "winmm:in:")?;
        let shared = Arc::new(InputShared {
            port,
            tx: self.tx.clone(),
            dropped: Arc::clone(&self.dropped),
            live: AtomicBool::new(true),
        });

        let mut handle = 0usize;
        let rc = unsafe {
            midiInOpen(
                &mut handle,
                device_id,
                midi_in_callback as *const () as usize,
                Arc::as_ptr(&shared) as usize,
                CALLBACK_FUNCTION,
            )
        };
        if rc != MMSYSERR_NOERROR {
            return Err(mm_error(rc, &id.0));
        }

        let mut buffers = Vec::with_capacity(SYSEX_BUFFERS);
        for _ in 0..SYSEX_BUFFERS {
            match prepare_sysex(handle) {
                Ok(buf) => buffers.push(buf),
                Err(err) => {
                    unsafe {
                        midiInReset(handle);
                        midiInClose(handle);
                    }
                    return Err(err);
                }
            }
        }

        let rc = unsafe { midiInStart(handle) };
        if rc != MMSYSERR_NOERROR {
            shared.live.store(false, Ordering::Release);
            unsafe {
                midiInReset(handle);
                for buf in &mut buffers {
                    midiInUnprepareHeader(handle, buf.hdr.as_mut(), size_of::<MidiHdr>() as u32);
                }
                midiInClose(handle);
            }
            return Err(mm_error(rc, &format!("midiInStart {}", id.0)));
        }

        self.parsers.entry(port).or_default();
        self.inputs.insert(
            id.0.clone(),
            OpenInput {
                handle,
                shared,
                _buffers: buffers,
            },
        );
        Ok(())
    }

    fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError> {
        let Some(mut input) = self.inputs.remove(&id.0) else {
            return Ok(());
        };
        input.shared.live.store(false, Ordering::Release);
        unsafe {
            midiInStop(input.handle);
            midiInReset(input.handle);
            for buf in &mut input._buffers {
                midiInUnprepareHeader(input.handle, buf.hdr.as_mut(), size_of::<MidiHdr>() as u32);
            }
            midiInClose(input.handle);
        }
        self.parsers.remove(&input.shared.port);
        Ok(())
    }

    fn open_output(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        if self.outputs.contains_key(&id.0) {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        let device_id = parse_index(&id.0, "winmm:out:")?;
        let mut handle = 0usize;
        let rc = unsafe { midiOutOpen(&mut handle, device_id, 0, 0, CALLBACK_NULL) };
        if rc != MMSYSERR_NOERROR {
            return Err(mm_error(rc, &id.0));
        }
        self.outputs.insert(id.0.clone(), OpenOutput { handle });
        Ok(())
    }

    fn close_output(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if let Some(output) = self.outputs.remove(&id.0) {
            unsafe {
                midiOutClose(output.handle);
            }
        }
        Ok(())
    }

    fn poll(&mut self, out: &mut Vec<MidiEvent>) -> u64 {
        while let Ok(frame) = self.rx.try_recv() {
            let time = Timestamp::from_nanos(u64::from(frame.time_ms) * 1_000_000);
            match frame.kind {
                FrameKind::Short(packed) => {
                    out.push(MidiEvent::new(
                        time,
                        frame.port,
                        ump_from_packed_short(packed),
                    ));
                }
                FrameKind::SysEx { len, buf } => {
                    let parser = self.parsers.entry(frame.port).or_default();
                    let packets = parser.push_slice(&buf[..usize::from(len)]);
                    for packet in packets {
                        out.push(MidiEvent::new(time, frame.port, packet));
                    }
                }
            }
        }
        self.dropped.load(Ordering::Relaxed)
    }

    fn send(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError> {
        let Some(output) = self.outputs.get(&id.0) else {
            return Err(IoError::NotFound(id.0.clone()));
        };
        let packed = packed_short_from_ump(packet).ok_or(IoError::UnsupportedPacket)?;
        let rc = unsafe { midiOutShortMsg(output.handle, packed) };
        if rc != MMSYSERR_NOERROR {
            return Err(mm_error(rc, &format!("midiOutShortMsg {}", id.0)));
        }
        Ok(())
    }
}

fn prepare_sysex(handle: usize) -> Result<PreparedSysex, IoError> {
    let mut data = Box::new([0u8; SYSEX_CAP]);
    let mut hdr = Box::new(MidiHdr::empty());
    hdr.lp_data = data.as_mut_ptr();
    hdr.buffer_length = SYSEX_CAP as u32;
    let rc = unsafe { midiInPrepareHeader(handle, hdr.as_mut(), size_of::<MidiHdr>() as u32) };
    if rc != MMSYSERR_NOERROR {
        return Err(mm_error(rc, "midiInPrepareHeader"));
    }
    let rc = unsafe { midiInAddBuffer(handle, hdr.as_mut(), size_of::<MidiHdr>() as u32) };
    if rc != MMSYSERR_NOERROR {
        unsafe {
            midiInUnprepareHeader(handle, hdr.as_mut(), size_of::<MidiHdr>() as u32);
        }
        return Err(mm_error(rc, "midiInAddBuffer"));
    }
    Ok(PreparedSysex { hdr, _data: data })
}

unsafe extern "system" fn midi_in_callback(
    handle: usize,
    msg: u32,
    instance: usize,
    param1: usize,
    param2: usize,
) {
    if instance == 0 {
        return;
    }
    let shared = unsafe { &*(instance as *const InputShared) };
    match msg {
        MIM_DATA => {
            push_frame(
                shared,
                CaptureFrame {
                    time_ms: param2 as u32,
                    port: shared.port,
                    kind: FrameKind::Short(param1 as u32),
                },
            );
        }
        MIM_LONGDATA if param1 != 0 => {
            let hdr = unsafe { &*(param1 as *const MidiHdr) };
            let mut buf = [0u8; SYSEX_CAP];
            let len = (hdr.bytes_recorded as usize).min(SYSEX_CAP);
            if !hdr.lp_data.is_null() && len > 0 {
                unsafe {
                    buf[..len].copy_from_slice(std::slice::from_raw_parts(hdr.lp_data, len));
                }
            }
            push_frame(
                shared,
                CaptureFrame {
                    time_ms: param2 as u32,
                    port: shared.port,
                    kind: FrameKind::SysEx {
                        len: len as u16,
                        buf,
                    },
                },
            );
            if shared.live.load(Ordering::Acquire) {
                unsafe {
                    midiInAddBuffer(handle, param1 as *mut MidiHdr, size_of::<MidiHdr>() as u32);
                }
            }
        }
        _ => {}
    }
}

fn push_frame(shared: &InputShared, frame: CaptureFrame) {
    match shared.tx.try_send(frame) {
        Ok(()) => {}
        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
            shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn enumerate() -> Result<Vec<Endpoint>, IoError> {
    let mut endpoints = Vec::new();

    let ins = unsafe { midiInGetNumDevs() };
    for i in 0..ins {
        let mut caps = MidiInCapsW {
            _w_mid: 0,
            _w_pid: 0,
            _driver_version: 0,
            name: [0; MAXPNAMELEN],
            _support: 0,
        };
        let rc =
            unsafe { midiInGetDevCapsW(i as usize, &mut caps, size_of::<MidiInCapsW>() as u32) };
        if rc != MMSYSERR_NOERROR {
            return Err(mm_error(rc, &format!("midiInGetDevCapsW({i})")));
        }
        endpoints.push(Endpoint {
            id: EndpointId(format!("winmm:in:{i}")),
            name: utf16_name(&caps.name),
            direction: Direction::Input,
            protocol: ProtocolHint::Midi1Bytes,
        });
    }

    let outs = unsafe { midiOutGetNumDevs() };
    for i in 0..outs {
        let mut caps = MidiOutCapsW {
            _w_mid: 0,
            _w_pid: 0,
            _driver_version: 0,
            name: [0; MAXPNAMELEN],
            _technology: 0,
            _voices: 0,
            _notes: 0,
            _channel_mask: 0,
            _support: 0,
        };
        let rc =
            unsafe { midiOutGetDevCapsW(i as usize, &mut caps, size_of::<MidiOutCapsW>() as u32) };
        if rc != MMSYSERR_NOERROR {
            return Err(mm_error(rc, &format!("midiOutGetDevCapsW({i})")));
        }
        endpoints.push(Endpoint {
            id: EndpointId(format!("winmm:out:{i}")),
            name: utf16_name(&caps.name),
            direction: Direction::Output,
            protocol: ProtocolHint::Midi1Bytes,
        });
    }

    Ok(endpoints)
}

fn parse_index(id: &str, prefix: &str) -> Result<usize, IoError> {
    id.strip_prefix(prefix)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| IoError::NotFound(id.to_string()))
}

fn mm_error(rc: u32, what: &str) -> IoError {
    match rc {
        MMSYSERR_ALLOCATED => IoError::InUse(what.to_string()),
        _ => IoError::Backend(format!("{what} failed (mmcode {rc})")),
    }
}

fn utf16_name(raw: &[u16]) -> String {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midihdr_layout_is_120_bytes() {
        assert_eq!(size_of::<MidiHdr>(), 120);
    }
}
