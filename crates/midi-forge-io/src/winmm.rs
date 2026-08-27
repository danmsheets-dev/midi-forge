//! WinMM enumeration, capture, and short-message output.

use std::collections::HashMap;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::time::{Duration, Instant};

use midi_forge_core::{
    Midi1Parser, MidiEvent, PortId, Timestamp, UmpMessage, packed_short_from_ump,
    ump_from_packed_short,
};

use crate::backend::{
    Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint, packets_for_wire,
};
use crate::error::IoError;
use crate::loopback::SoftwareLoopbacks;

const MAXPNAMELEN: usize = 32;
const MMSYSERR_NOERROR: u32 = 0;
const MMSYSERR_ALLOCATED: u32 = 4;
const MIDIERR_STILLPLAYING: u32 = 65;
const CALLBACK_NULL: u32 = 0;
const CALLBACK_FUNCTION: u32 = 0x0003_0000;
const MIM_DATA: u32 = 0x3C3;
const MIM_LONGDATA: u32 = 0x3C4;
const MIM_LONGERROR: u32 = 0x3C6;
const SYSEX_BUFFERS: usize = 8;
const SYSEX_CAP: usize = 16 * 1024;
const QUEUE_CAP: usize = 4096;
const MHDR_DONE: u32 = 0x0000_0001;
fn sysex_send_timeout(len: usize) -> Duration {
    let ms = 2_000 + (len as u64).saturating_mul(1000) / 2_500;
    Duration::from_millis(ms.min(60_000))
}

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
    fn midiOutReset(handle: usize) -> u32;
    fn midiOutShortMsg(handle: usize, msg: u32) -> u32;
    fn midiOutPrepareHeader(handle: usize, header: *mut MidiHdr, size: u32) -> u32;
    fn midiOutLongMsg(handle: usize, header: *mut MidiHdr, size: u32) -> u32;
    fn midiOutUnprepareHeader(handle: usize, header: *mut MidiHdr, size: u32) -> u32;
}

struct CaptureFrame {
    time_ms: u32,
    port: PortId,
    kind: FrameKind,
}

const SYSEX_CHUNK: usize = 256;

#[derive(Clone, Copy)]
enum FrameKind {
    Short(u32),
    SysEx { len: u16, data: [u8; SYSEX_CHUNK] },
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
    /// Extra Arc clone given to WinMM as dwInstance (`Arc::into_raw`).
    instance: usize,
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
    loopbacks: SoftwareLoopbacks,
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
            loopbacks: SoftwareLoopbacks::new(),
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
        if midisrv_running() {
            "winmm+midisrv"
        } else {
            "winmm"
        }
    }

    fn refresh(&mut self) -> Result<(), IoError> {
        self.rebuild_endpoints()
    }

    fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    fn open_input(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.open_input(id, port);
        }
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
        let instance = Arc::into_raw(Arc::clone(&shared)) as usize;
        let rc = unsafe {
            midiInOpen(
                &mut handle,
                device_id,
                midi_in_callback as *const () as usize,
                instance,
                CALLBACK_FUNCTION,
            )
        };
        if rc != MMSYSERR_NOERROR {
            unsafe {
                drop(Arc::from_raw(instance as *const InputShared));
            }
            return Err(mm_error(rc, &id.0));
        }

        let mut buffers = Vec::with_capacity(SYSEX_BUFFERS);
        for _ in 0..SYSEX_BUFFERS {
            match prepare_sysex(handle) {
                Ok(buf) => buffers.push(buf),
                Err(err) => {
                    shutdown_input(handle, &shared, &mut buffers);
                    unsafe {
                        drop(Arc::from_raw(instance as *const InputShared));
                    }
                    return Err(err);
                }
            }
        }

        let rc = unsafe { midiInStart(handle) };
        if rc != MMSYSERR_NOERROR {
            shutdown_input(handle, &shared, &mut buffers);
            unsafe {
                drop(Arc::from_raw(instance as *const InputShared));
            }
            return Err(mm_error(rc, &format!("midiInStart {}", id.0)));
        }

        self.parsers.entry(port).or_default();
        self.inputs.insert(
            id.0.clone(),
            OpenInput {
                handle,
                shared,
                instance,
                _buffers: buffers,
            },
        );
        Ok(())
    }

    fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.close_input(id);
        }
        let Some(mut input) = self.inputs.remove(&id.0) else {
            return Ok(());
        };
        shutdown_input(input.handle, &input.shared, &mut input._buffers);
        unsafe {
            drop(Arc::from_raw(input.instance as *const InputShared));
        }
        self.parsers.remove(&input.shared.port);
        Ok(())
    }

    fn open_output(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.open_output(id);
        }
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
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.close_output(id);
        }
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
                FrameKind::SysEx { len, data } => {
                    let parser = self.parsers.entry(frame.port).or_default();
                    let n = usize::from(len).min(SYSEX_CHUNK);
                    let packets = parser.push_slice(&data[..n]);
                    for packet in packets {
                        out.push(MidiEvent::new(time, frame.port, packet));
                    }
                }
            }
        }
        let loop_dropped = self.loopbacks.poll(out);
        self.dropped.load(Ordering::Relaxed) + loop_dropped
    }

    fn send(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.send(id, *packet);
        }
        let Some(output) = self.outputs.get(&id.0) else {
            return Err(IoError::NotFound(id.0.clone()));
        };
        let handle = output.handle;
        let protocol = self
            .endpoints
            .iter()
            .find(|e| e.id == *id)
            .map(|e| e.protocol)
            .unwrap_or(ProtocolHint::Midi1Bytes);
        for packet in packets_for_wire(protocol, packet) {
            let packed = packed_short_from_ump(&packet).ok_or(IoError::UnsupportedPacket)?;
            let rc = unsafe { midiOutShortMsg(handle, packed) };
            if rc != MMSYSERR_NOERROR {
                return Err(mm_error(rc, &format!("midiOutShortMsg {}", id.0)));
            }
        }
        Ok(())
    }

    fn send_sysex(&mut self, id: &EndpointId, bytes: &[u8]) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.send_sysex(id, bytes);
        }
        let Some(output) = self.outputs.get(&id.0) else {
            return Err(IoError::NotFound(id.0.clone()));
        };
        if bytes.first() != Some(&0xF0) || bytes.last() != Some(&0xF7) {
            return Err(IoError::UnsupportedPacket);
        }
        send_long_message(output.handle, bytes)
    }

    fn create_loopback(&mut self, name: &str) -> Result<(EndpointId, EndpointId), IoError> {
        let pair = self.loopbacks.create(name);
        self.rebuild_endpoints()?;
        Ok(pair)
    }

    fn remove_loopback(&mut self, id: &EndpointId) -> Result<(), IoError> {
        self.loopbacks.remove(id)?;
        self.rebuild_endpoints()
    }

    fn caps(&self) -> crate::backend::BackendCaps {
        crate::backend::BackendCaps {
            native_ump: false,
            scheduled_send: false,
            daw_visible_virtual: false,
            multi_client: midisrv_running(),
        }
    }
}

// MidiHdr contains raw pointers; access is serialized by the engine mutex.
unsafe impl Send for PreparedSysex {}
unsafe impl Send for WinMmBackend {}

impl WinMmBackend {
    fn rebuild_endpoints(&mut self) -> Result<(), IoError> {
        let mut endpoints = enumerate()?;
        endpoints.extend(self.loopbacks.endpoints());
        self.endpoints = endpoints;
        Ok(())
    }
}

fn send_long_message(handle: usize, bytes: &[u8]) -> Result<(), IoError> {
    let mut data = bytes.to_vec();
    let mut hdr = Box::new(MidiHdr::empty());
    hdr.lp_data = data.as_mut_ptr();
    hdr.buffer_length = data.len() as u32;
    let hdr_size = size_of::<MidiHdr>() as u32;
    let rc = unsafe { midiOutPrepareHeader(handle, hdr.as_mut(), hdr_size) };
    if rc != MMSYSERR_NOERROR {
        return Err(mm_error(rc, "midiOutPrepareHeader"));
    }
    let rc = unsafe { midiOutLongMsg(handle, hdr.as_mut(), hdr_size) };
    if rc != MMSYSERR_NOERROR {
        let _ = wait_unprepare_out(handle, hdr.as_mut(), hdr_size);
        return Err(mm_error(rc, "midiOutLongMsg"));
    }
    let deadline = Instant::now() + sysex_send_timeout(bytes.len());
    while hdr.flags & MHDR_DONE == 0 {
        if Instant::now() > deadline {
            unsafe {
                midiOutReset(handle);
            }
            let done_deadline = Instant::now() + Duration::from_millis(250);
            while hdr.flags & MHDR_DONE == 0 && Instant::now() < done_deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            let rc = wait_unprepare_out(handle, hdr.as_mut(), hdr_size);
            if rc == MIDIERR_STILLPLAYING {
                std::mem::forget(data);
                std::mem::forget(hdr);
                return Err(IoError::Backend(
                    "SysEx send timed out; buffer leaked until driver releases it".into(),
                ));
            }
            return Err(IoError::Backend("SysEx send timed out".into()));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let rc = wait_unprepare_out(handle, hdr.as_mut(), hdr_size);
    if rc != MMSYSERR_NOERROR {
        if rc == MIDIERR_STILLPLAYING {
            std::mem::forget(data);
            std::mem::forget(hdr);
        }
        return Err(mm_error(rc, "midiOutUnprepareHeader"));
    }
    Ok(())
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

fn shutdown_input(handle: usize, shared: &InputShared, buffers: &mut [PreparedSysex]) {
    shared.live.store(false, Ordering::Release);
    unsafe {
        midiInStop(handle);
        midiInReset(handle);
    }
    // Let in-flight CALLBACK_FUNCTION calls observe live=false before we unprepare.
    std::thread::sleep(Duration::from_millis(2));
    let hdr_size = size_of::<MidiHdr>() as u32;
    for buf in buffers.iter_mut() {
        let _ = wait_unprepare_in(handle, buf.hdr.as_mut(), hdr_size);
    }
    wait_close_in(handle);
}

fn wait_unprepare_in(handle: usize, hdr: *mut MidiHdr, hdr_size: u32) -> u32 {
    for _ in 0..250 {
        let rc = unsafe { midiInUnprepareHeader(handle, hdr, hdr_size) };
        if rc != MIDIERR_STILLPLAYING {
            return rc;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    MIDIERR_STILLPLAYING
}

fn wait_close_in(handle: usize) {
    for _ in 0..250 {
        let rc = unsafe { midiInClose(handle) };
        if rc != MIDIERR_STILLPLAYING {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_unprepare_out(handle: usize, hdr: *mut MidiHdr, hdr_size: u32) -> u32 {
    for _ in 0..250 {
        let rc = unsafe { midiOutUnprepareHeader(handle, hdr, hdr_size) };
        if rc != MIDIERR_STILLPLAYING {
            return rc;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    MIDIERR_STILLPLAYING
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
        MIM_DATA if shared.live.load(Ordering::Acquire) => {
            push_frame(
                shared,
                CaptureFrame {
                    time_ms: param2 as u32,
                    port: shared.port,
                    kind: FrameKind::Short(param1 as u32),
                },
            );
        }
        MIM_LONGDATA | MIM_LONGERROR if param1 != 0 => {
            if msg == MIM_LONGDATA && shared.live.load(Ordering::Acquire) {
                let hdr = unsafe { &*(param1 as *const MidiHdr) };
                let total = (hdr.bytes_recorded as usize).min(SYSEX_CAP);
                if !hdr.lp_data.is_null() && total > 0 {
                    let src = unsafe { std::slice::from_raw_parts(hdr.lp_data, total) };
                    let mut offset = 0;
                    while offset < src.len() {
                        let mut data = [0u8; SYSEX_CHUNK];
                        let n = (src.len() - offset).min(SYSEX_CHUNK);
                        data[..n].copy_from_slice(&src[offset..offset + n]);
                        push_frame(
                            shared,
                            CaptureFrame {
                                time_ms: param2 as u32,
                                port: shared.port,
                                kind: FrameKind::SysEx {
                                    len: n as u16,
                                    data,
                                },
                            },
                        );
                        offset += n;
                    }
                }
            }
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
            continue;
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
            continue;
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

/// True when the Windows MIDI Services process (`MidiSrv`) is installed and running.
/// UMP I/O still needs the App SDK; WinMM is remapped through the service when this is true.
pub fn midisrv_running() -> bool {
    midisrv_running_named("MidiSrv") || midisrv_running_named("Midisrv")
}

fn midisrv_running_named(name: &str) -> bool {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    unsafe {
        let scm = OpenSCManagerW(ptr::null(), ptr::null(), SC_MANAGER_CONNECT);
        if scm == 0 {
            return false;
        }
        let svc = OpenServiceW(scm, wide.as_ptr(), SERVICE_QUERY_STATUS);
        if svc == 0 {
            CloseServiceHandle(scm);
            return false;
        }
        let mut status = ServiceStatus::default();
        let ok = QueryServiceStatus(svc, &mut status) != 0;
        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
        ok && status.current_state == SERVICE_RUNNING
    }
}

const SC_MANAGER_CONNECT: u32 = 0x0001;
const SERVICE_QUERY_STATUS: u32 = 0x0004;
const SERVICE_RUNNING: u32 = 0x0000_0004;

#[repr(C)]
#[derive(Default)]
struct ServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> isize;
    fn OpenServiceW(manager: isize, name: *const u16, access: u32) -> isize;
    fn CloseServiceHandle(handle: isize) -> i32;
    fn QueryServiceStatus(service: isize, status: *mut ServiceStatus) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stillplaying_is_midierr_base_plus_one() {
        assert_eq!(MIDIERR_STILLPLAYING, 65);
        assert_eq!(MIM_LONGERROR, 0x3C6);
    }

    #[test]
    fn midihdr_layout_is_120_bytes() {
        assert_eq!(size_of::<MidiHdr>(), 120);
    }

    #[test]
    fn midisrv_probe_does_not_panic() {
        let _ = midisrv_running();
    }
}
