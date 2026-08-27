//! Windows MIDI Services App SDK bootstrap + live `MidiSession` backend.
//!
//! `WmsInit` CoCreates `MidiDesktopAppSdkInitializer` (MTA). Bindings come from
//! the vendored winmd via `windows-bindgen`. Runtime still needs the user's
//! installed App SDK projection; [`WmsBackend::try_new`] returns `Err` (WinMM
//! fallback) if WinRT activation fails. Never fakes `native_ump`.

#![cfg(windows)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

use midi_forge_core::{MidiEvent, PortId, SysexDump, Timestamp, UmpMessage};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::WinRT::RoGetActivationFactory;
use windows::core::{GUID, HRESULT, IUnknown, Interface};

use crate::backend::{
    BackendCaps, Endpoint, EndpointId, MidiBackend, ProtocolHint, packets_for_wire,
};
use crate::error::IoError;
use crate::loopback::SoftwareLoopbacks;

#[cfg(midi2_has_winmd)]
mod winrt {
    include!(concat!(env!("OUT_DIR"), "/wms_winrt.rs"));
}

const SDK_INSTALL: &str = "Install SDK: winget install Microsoft.WindowsMIDIServicesSDK";
const SESSION_NAME: &str = "Midi-Forge";
const QUEUE_CAP: usize = 4096;
const IN_PREFIX: &str = "wms:in:";
const OUT_PREFIX: &str = "wms:out:";

const CLSID_MIDI_DESKTOP_APP_SDK_INITIALIZER: GUID =
    GUID::from_u128(0xc3263827_c3b0_bdbd_2500_ce63a3f3f2c3);
const IID_I_MIDI_CLIENT_INITIALIZER: GUID = GUID::from_u128(0x8087b303_d551_bce2_1ead_a2500d50c580);
const IID_CONNECTION_RAW: GUID = GUID::from_u128(0x8087b303_0519_31d1_31d1_000000000020);
const IID_MESSAGES_RECEIVED: GUID = GUID::from_u128(0x8087b303_0519_31d1_31d1_000000000010);
const IID_IUNKNOWN: GUID = GUID::from_u128(0x00000000_0000_0000_c000_000000000046);

fn sdk_err() -> IoError {
    IoError::Backend(SDK_INSTALL.into())
}

fn session_err(detail: &str) -> IoError {
    IoError::Backend(format!("{detail}. {SDK_INSTALL}"))
}

/// Owns MTA COM and the App SDK initializer. Drop uninitializes COM.
pub struct WmsInit {
    _initializer: IUnknown,
}

// MTA COM: the engine thread owns this. IUnknown is !Send for STA; we never
// leave the MTA apartment we initialized.
unsafe impl Send for WmsInit {}

impl WmsInit {
    pub fn try_new() -> Result<Self, IoError> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|_| sdk_err())?;
        }

        let constructed = (|| unsafe {
            let unk: IUnknown = CoCreateInstance(
                &CLSID_MIDI_DESKTOP_APP_SDK_INITIALIZER,
                None,
                CLSCTX_INPROC_SERVER,
            )
            .map_err(|_| sdk_err())?;

            let mut init_ptr = ptr::null_mut();
            unk.query(&IID_I_MIDI_CLIENT_INITIALIZER, &mut init_ptr)
                .ok()
                .map_err(|_| sdk_err())?;
            if init_ptr.is_null() {
                return Err(sdk_err());
            }
            let initializer = IUnknown::from_raw(init_ptr);

            // Slot 3 = GetInstalledWindowsMidiServicesSdkVersion (InitializeSdkRuntime).
            // Slot 4 = EnsureServiceAvailable.
            let mut major = 0u16;
            let mut minor = 0u16;
            let mut patch = 0u16;
            let ver_hr = vcall::<
                unsafe extern "system" fn(
                    *mut c_void,
                    *mut u32,
                    *mut u16,
                    *mut u16,
                    *mut u16,
                    *mut *mut u16,
                    *mut *mut u16,
                    *mut *mut u16,
                ) -> HRESULT,
            >(initializer.as_raw(), 3)(
                initializer.as_raw(),
                ptr::null_mut(),
                &mut major,
                &mut minor,
                &mut patch,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            let svc_hr = vcall::<unsafe extern "system" fn(*mut c_void) -> HRESULT>(
                initializer.as_raw(),
                4,
            )(initializer.as_raw());
            if ver_hr.is_err() && svc_hr.is_err() {
                return Err(sdk_err());
            }
            Ok(Self {
                _initializer: initializer,
            })
        })();

        match constructed {
            Ok(init) => Ok(init),
            Err(err) => {
                unsafe {
                    CoUninitialize();
                }
                Err(err)
            }
        }
    }
}

impl Drop for WmsInit {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

unsafe fn vcall<F: Copy>(this: *mut c_void, slot: usize) -> F {
    let vtbl = this as *const *const *const c_void;
    let f = unsafe { *(*vtbl).add(slot) };
    unsafe { std::mem::transmute_copy(&f) }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WmsDir {
    Input,
    Output,
}

fn parse_wms_id(id: &str) -> Option<(WmsDir, &str)> {
    if let Some(rest) = id.strip_prefix(IN_PREFIX) {
        return Some((WmsDir::Input, rest));
    }
    if let Some(rest) = id.strip_prefix(OUT_PREFIX) {
        return Some((WmsDir::Output, rest));
    }
    None
}

fn in_id(device: &str) -> EndpointId {
    EndpointId(format!("{IN_PREFIX}{device}"))
}

fn out_id(device: &str) -> EndpointId {
    EndpointId(format!("{OUT_PREFIX}{device}"))
}

/// Split a packed UMP word stream into complete packets. Stops on a truncated
/// or unknown message type so a bad word cannot desync the rest forever.
fn split_ump_words(mut words: &[u32]) -> Vec<UmpMessage> {
    let mut out = Vec::new();
    while !words.is_empty() {
        match UmpMessage::try_from_words(words) {
            Ok(msg) => {
                let n = msg.len();
                words = &words[n..];
                out.push(msg);
            }
            Err(_) => break,
        }
    }
    out
}

fn winrt_class_available(name: &str) -> bool {
    let class: windows::core::HSTRING = name.into();
    unsafe { RoGetActivationFactory::<windows::core::IInspectable>(&class).is_ok() }
}

struct CaptureFrame {
    time_100ns: u64,
    port: PortId,
    packet: UmpMessage,
}

/// Native UMP backend over `MidiSession`.
pub struct WmsBackend {
    session: Option<WinrtSession>,
    endpoints: Vec<Endpoint>,
    conns: HashMap<String, OpenConn>,
    tx: SyncSender<CaptureFrame>,
    rx: Receiver<CaptureFrame>,
    dropped: Arc<AtomicU64>,
    loopbacks: SoftwareLoopbacks,
    _init: WmsInit,
}

struct WinrtSession {
    inner: winrt_session::Session,
}

struct OpenConn {
    inner: winrt_session::Connection,
    raw: *mut c_void,
    _callback: Option<Box<ReceiveSink>>,
    in_port: Option<PortId>,
    out_open: bool,
}

unsafe impl Send for OpenConn {}

impl WmsBackend {
    pub fn try_new() -> Result<Self, IoError> {
        let init = WmsInit::try_new()?;
        #[cfg(not(midi2_has_winmd))]
        {
            let _ = init;
            return Err(session_err(
                "MidiSession projection not bound (no SDK winmd)",
            ));
        }
        #[cfg(midi2_has_winmd)]
        {
            if !winrt_class_available("Windows.Devices.Midi2.MidiSession") {
                return Err(session_err("MidiSession WinRT class is not registered"));
            }
            if !winrt_session::ensure_service() {
                return Err(session_err("Windows MIDI Services is not available"));
            }
            let session = winrt_session::Session::create(SESSION_NAME)
                .ok_or_else(|| session_err("MidiSession::Create returned null"))?;
            let (tx, rx) = sync_channel(QUEUE_CAP);
            let mut this = Self {
                session: Some(WinrtSession { inner: session }),
                endpoints: Vec::new(),
                conns: HashMap::new(),
                tx,
                rx,
                dropped: Arc::new(AtomicU64::new(0)),
                loopbacks: SoftwareLoopbacks::new(),
                _init: init,
            };
            let _ = this.refresh();
            Ok(this)
        }
    }

    fn session(&self) -> Result<&winrt_session::Session, IoError> {
        self.session
            .as_ref()
            .map(|s| &s.inner)
            .ok_or_else(|| session_err("no MidiSession"))
    }

    fn ensure_conn(&mut self, device: &str) -> Result<&mut OpenConn, IoError> {
        if !self.conns.contains_key(device) {
            let session = self.session()?;
            let conn = session
                .connect(device)
                .ok_or_else(|| IoError::NotFound(device.into()))?;
            let raw = conn.query_raw().ok_or_else(|| {
                session_err("IMidiEndpointConnectionRaw is not available on this connection")
            })?;
            if !conn.open() {
                unsafe {
                    raw.release();
                }
                return Err(IoError::Backend(format!("failed to open {device}")));
            }
            self.conns.insert(
                device.to_string(),
                OpenConn {
                    inner: conn,
                    raw,
                    _callback: None,
                    in_port: None,
                    out_open: false,
                },
            );
        }
        Ok(self.conns.get_mut(device).expect("just inserted"))
    }

    fn close_conn_if_idle(&mut self, device: &str) {
        let idle = self
            .conns
            .get(device)
            .is_some_and(|c| c.in_port.is_none() && !c.out_open);
        if idle {
            if let Some(mut conn) = self.conns.remove(device) {
                conn.detach();
                if let Ok(session) = self.session() {
                    session.disconnect(conn.inner.connection_id());
                }
            }
        }
    }
}

impl Drop for WmsBackend {
    fn drop(&mut self) {
        let devices: Vec<String> = self.conns.keys().cloned().collect();
        for device in devices {
            if let Some(mut conn) = self.conns.remove(&device) {
                conn.detach();
                if let Ok(session) = self.session() {
                    session.disconnect(conn.inner.connection_id());
                }
            }
        }
        if let Some(session) = self.session.take() {
            session.inner.close();
        }
    }
}

impl MidiBackend for WmsBackend {
    fn name(&self) -> &'static str {
        "wms"
    }

    fn refresh(&mut self) -> Result<(), IoError> {
        let mut endpoints = winrt_session::enumerate().unwrap_or_default();
        endpoints.extend(self.loopbacks.endpoints());
        self.endpoints = endpoints;
        Ok(())
    }

    fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }

    fn open_input(&mut self, id: &EndpointId, port: PortId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.open_input(id, port);
        }
        let (dir, device) = parse_wms_id(&id.0).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        if dir != WmsDir::Input {
            return Err(IoError::NotFound(id.0.clone()));
        }
        let device = device.to_string();
        let tx = self.tx.clone();
        let dropped = Arc::clone(&self.dropped);
        let conn = self.ensure_conn(&device)?;
        if conn.in_port.is_some() {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        let sink = ReceiveSink::new(tx, port, dropped);
        let hr = unsafe { conn.raw.set_callback(sink.as_com()) };
        if hr.is_err() {
            return Err(IoError::Backend(format!(
                "SetMessagesReceivedCallback failed: {hr:?}"
            )));
        }
        conn._callback = Some(sink);
        conn.in_port = Some(port);
        Ok(())
    }

    fn close_input(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.close_input(id);
        }
        let Some((WmsDir::Input, device)) = parse_wms_id(&id.0) else {
            return Ok(());
        };
        let device = device.to_string();
        if let Some(conn) = self.conns.get_mut(&device) {
            unsafe {
                let _ = conn.raw.remove_callback();
            }
            conn._callback = None;
            conn.in_port = None;
        }
        self.close_conn_if_idle(&device);
        Ok(())
    }

    fn open_output(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.open_output(id);
        }
        let (dir, device) = parse_wms_id(&id.0).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        if dir != WmsDir::Output {
            return Err(IoError::NotFound(id.0.clone()));
        }
        let device = device.to_string();
        let conn = self.ensure_conn(&device)?;
        if conn.out_open {
            return Err(IoError::AlreadyOpen(id.0.clone()));
        }
        conn.out_open = true;
        Ok(())
    }

    fn close_output(&mut self, id: &EndpointId) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.close_output(id);
        }
        let Some((WmsDir::Output, device)) = parse_wms_id(&id.0) else {
            return Ok(());
        };
        let device = device.to_string();
        if let Some(conn) = self.conns.get_mut(&device) {
            conn.out_open = false;
        }
        self.close_conn_if_idle(&device);
        Ok(())
    }

    fn poll(&mut self, out: &mut Vec<MidiEvent>) -> u64 {
        let mut dropped = self.loopbacks.poll(out);
        while let Ok(frame) = self.rx.try_recv() {
            out.push(MidiEvent::new(
                Timestamp::from_nanos(frame.time_100ns.saturating_mul(100)),
                frame.port,
                frame.packet,
            ));
        }
        dropped += self.dropped.swap(0, Ordering::Relaxed);
        dropped
    }

    fn send(&mut self, id: &EndpointId, packet: &UmpMessage) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.send(id, *packet);
        }
        let (dir, device) = parse_wms_id(&id.0).ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        if dir != WmsDir::Output {
            return Err(IoError::NotFound(id.0.clone()));
        }
        let device = device.to_string();
        let conn = self
            .conns
            .get_mut(&device)
            .ok_or_else(|| IoError::NotFound(id.0.clone()))?;
        if !conn.out_open {
            return Err(IoError::NotFound(id.0.clone()));
        }
        for wire in packets_for_wire(ProtocolHint::Ump, packet) {
            let words = wire.words();
            let hr = unsafe { conn.raw.send_words(0, words) };
            if hr.is_err() {
                return Err(IoError::Backend(format!(
                    "SendMidiMessagesRaw failed: {hr:?}"
                )));
            }
        }
        Ok(())
    }

    fn send_sysex(&mut self, id: &EndpointId, bytes: &[u8]) -> Result<(), IoError> {
        if self.loopbacks.is_ours(id) {
            return self.loopbacks.send_sysex(id, bytes);
        }
        let dump = SysexDump::from_bytes(bytes.to_vec()).map_err(|_| IoError::UnsupportedPacket)?;
        for packet in dump.to_ump_packets(0) {
            self.send(id, &packet)?;
        }
        Ok(())
    }

    fn create_loopback(&mut self, name: &str) -> Result<(EndpointId, EndpointId), IoError> {
        let pair = self.loopbacks.create(name);
        let _ = self.refresh();
        Ok(pair)
    }

    fn remove_loopback(&mut self, id: &EndpointId) -> Result<(), IoError> {
        self.loopbacks.remove(id)?;
        let _ = self.refresh();
        Ok(())
    }

    fn caps(&self) -> BackendCaps {
        BackendCaps {
            native_ump: true,
            scheduled_send: false,
            daw_visible_virtual: true,
            multi_client: true,
        }
    }
}

impl OpenConn {
    fn detach(&mut self) {
        unsafe {
            let _ = self.raw.remove_callback();
            self.raw.release();
        }
        self.raw = ptr::null_mut();
        self._callback = None;
        self.in_port = None;
        self.out_open = false;
    }
}

/// COM `IMidiEndpointConnectionRaw` (local, pointer-friendly send/receive).
/// Vtable after IUnknown: 3 GetMax, 4 Validate, 5 Send, 6 SetCallback, 7 Remove.
trait RawCalls {
    unsafe fn send_words(&self, timestamp: u64, words: &[u32]) -> HRESULT;
    unsafe fn set_callback(&self, cb: *mut c_void) -> HRESULT;
    unsafe fn remove_callback(&self) -> HRESULT;
    unsafe fn release(&self);
}

impl RawCalls for *mut c_void {
    unsafe fn send_words(&self, timestamp: u64, words: &[u32]) -> HRESULT {
        let this = *self;
        if this.is_null() {
            return HRESULT(0x8000_4003u32 as i32); // E_POINTER
        }
        unsafe {
            vcall::<unsafe extern "system" fn(*mut c_void, u64, u32, *mut u32) -> HRESULT>(this, 5)(
                this,
                timestamp,
                words.len() as u32,
                words.as_ptr() as *mut u32,
            )
        }
    }

    unsafe fn set_callback(&self, cb: *mut c_void) -> HRESULT {
        let this = *self;
        if this.is_null() {
            return HRESULT(0x8000_4003u32 as i32);
        }
        unsafe {
            vcall::<unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT>(this, 6)(
                this, cb,
            )
        }
    }

    unsafe fn remove_callback(&self) -> HRESULT {
        let this = *self;
        if this.is_null() {
            return HRESULT(0);
        }
        unsafe { vcall::<unsafe extern "system" fn(*mut c_void) -> HRESULT>(this, 7)(this) }
    }

    unsafe fn release(&self) {
        let this = *self;
        if this.is_null() {
            return;
        }
        unsafe {
            let _ = vcall::<unsafe extern "system" fn(*mut c_void) -> u32>(this, 2)(this);
        }
    }
}

#[repr(C)]
struct ReceiveSink {
    vtbl: *const ReceiveSinkVtbl,
    refcnt: AtomicU32,
    tx: SyncSender<CaptureFrame>,
    port: PortId,
    dropped: Arc<AtomicU64>,
}

#[repr(C)]
struct ReceiveSinkVtbl {
    query_interface: unsafe extern "system" fn(
        this: *mut c_void,
        iid: *const GUID,
        out: *mut *mut c_void,
    ) -> HRESULT,
    add_ref: unsafe extern "system" fn(this: *mut c_void) -> u32,
    release: unsafe extern "system" fn(this: *mut c_void) -> u32,
    messages_received: unsafe extern "system" fn(
        this: *mut c_void,
        session_id: GUID,
        connection_id: GUID,
        timestamp: u64,
        word_count: u32,
        messages: *mut u32,
    ) -> HRESULT,
}

static RECEIVE_VTBL: ReceiveSinkVtbl = ReceiveSinkVtbl {
    query_interface: sink_query_interface,
    add_ref: sink_add_ref,
    release: sink_release,
    messages_received: sink_messages_received,
};

impl ReceiveSink {
    fn new(tx: SyncSender<CaptureFrame>, port: PortId, dropped: Arc<AtomicU64>) -> Box<Self> {
        Box::new(Self {
            vtbl: &RECEIVE_VTBL,
            refcnt: AtomicU32::new(1),
            tx,
            port,
            dropped,
        })
    }

    fn as_com(&self) -> *mut c_void {
        self as *const Self as *mut c_void
    }
}

unsafe extern "system" fn sink_query_interface(
    this: *mut c_void,
    iid: *const GUID,
    out: *mut *mut c_void,
) -> HRESULT {
    if iid.is_null() || out.is_null() {
        return HRESULT(0x8000_4003u32 as i32);
    }
    let iid = unsafe { *iid };
    if iid == IID_IUNKNOWN || iid == IID_MESSAGES_RECEIVED {
        unsafe {
            *out = this;
        }
        unsafe {
            sink_add_ref(this);
        }
        HRESULT(0)
    } else {
        unsafe {
            *out = ptr::null_mut();
        }
        HRESULT(0x8000_4002u32 as i32) // E_NOINTERFACE
    }
}

unsafe extern "system" fn sink_add_ref(this: *mut c_void) -> u32 {
    let sink = unsafe { &*(this as *const ReceiveSink) };
    sink.refcnt.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn sink_release(this: *mut c_void) -> u32 {
    let sink = unsafe { &*(this as *const ReceiveSink) };
    let n = sink.refcnt.fetch_sub(1, Ordering::Release) - 1;
    n
}

unsafe extern "system" fn sink_messages_received(
    this: *mut c_void,
    _session_id: GUID,
    _connection_id: GUID,
    timestamp: u64,
    word_count: u32,
    messages: *mut u32,
) -> HRESULT {
    if messages.is_null() || word_count == 0 {
        return HRESULT(0);
    }
    let sink = unsafe { &*(this as *const ReceiveSink) };
    let words = unsafe { std::slice::from_raw_parts(messages, word_count as usize) };
    for packet in split_ump_words(words) {
        match sink.tx.try_send(CaptureFrame {
            time_100ns: timestamp,
            port: sink.port,
            packet,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                sink.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    HRESULT(0)
}

#[cfg(midi2_has_winmd)]
mod winrt_session {
    use super::*;
    use crate::backend::{Direction, Endpoint, ProtocolHint};

    use winrt::Windows::Devices::Midi2::Enumeration::{
        MidiEndpointDeviceInformation, MidiEndpointDeviceInformationFilters,
        MidiEndpointDeviceInformationSortOrder, MidiEndpointNativeDataFormat,
    };
    use winrt::Windows::Devices::Midi2::{MidiApi, MidiEndpointConnection, MidiSession};

    pub fn ensure_service() -> bool {
        MidiApi::EnsureServiceAvailable()
    }

    pub struct Session(MidiSession);
    pub struct Connection(MidiEndpointConnection);

    impl Session {
        pub fn create(name: &str) -> Option<Self> {
            let h: windows_core::HSTRING = name.into();
            MidiSession::Create(&h).map(Self)
        }

        pub fn connect(&self, device_id: &str) -> Option<Connection> {
            let h: windows_core::HSTRING = device_id.into();
            self.0.CreateEndpointConnection(&h).map(Connection)
        }

        pub fn disconnect(&self, id: windows_core::GUID) {
            self.0.DisconnectEndpointConnection(id);
        }

        pub fn close(&self) {
            let _ = self.0.Close();
        }
    }

    impl Connection {
        pub fn open(&self) -> bool {
            self.0.Open()
        }

        pub fn connection_id(&self) -> windows_core::GUID {
            self.0.ConnectionId()
        }

        pub fn query_raw(&self) -> Option<*mut c_void> {
            let unk: windows_core::IUnknown = windows_core::Interface::cast(&self.0).ok()?;
            let mut ptr = ptr::null_mut();
            let hr = unsafe { unk.query(&IID_CONNECTION_RAW, &mut ptr) };
            if hr.is_err() || ptr.is_null() {
                None
            } else {
                Some(ptr)
            }
        }
    }

    pub fn enumerate() -> Option<Vec<Endpoint>> {
        let filter = MidiEndpointDeviceInformationFilters::AllStandardEndpoints
            | MidiEndpointDeviceInformationFilters::DiagnosticLoopback;
        let view = MidiEndpointDeviceInformation::FindAll3(
            MidiEndpointDeviceInformationSortOrder::Name,
            filter,
        )?;
        let n = view.Size().ok()?;
        let mut out = Vec::with_capacity((n as usize).saturating_mul(2));
        for i in 0..n {
            let info = view.GetAt(i).ok()?;
            let device = info.EndpointDeviceId().to_string();
            if device.is_empty() {
                continue;
            }
            let name = info.Name().to_string();
            let protocol = match info.GetTransportSuppliedInfo() {
                Some(t)
                    if t.NativeDataFormat() == MidiEndpointNativeDataFormat::Midi1ByteFormat =>
                {
                    // Service still speaks UMP to the app; native wire may be MIDI 1.
                    ProtocolHint::Ump
                }
                _ => ProtocolHint::Ump,
            };
            out.push(Endpoint {
                id: in_id(&device),
                name: format!("{name} In"),
                direction: Direction::Input,
                protocol,
            });
            out.push(Endpoint {
                id: out_id(&device),
                name: format!("{name} Out"),
                direction: Direction::Output,
                protocol,
            });
        }
        Some(out)
    }
}

#[cfg(not(midi2_has_winmd))]
mod winrt_session {
    use super::*;
    use crate::backend::Endpoint;

    pub fn ensure_service() -> bool {
        false
    }

    pub struct Session;
    pub struct Connection;

    impl Session {
        pub fn create(_: &str) -> Option<Self> {
            None
        }
        pub fn connect(&self, _: &str) -> Option<Connection> {
            None
        }
        pub fn disconnect(&self, _: windows_core::GUID) {}
        pub fn close(&self) {}
    }

    impl Connection {
        pub fn open(&self) -> bool {
            false
        }
        pub fn connection_id(&self) -> windows_core::GUID {
            windows_core::GUID::zeroed()
        }
        pub fn query_raw(&self) -> Option<*mut c_void> {
            None
        }
    }

    pub fn enumerate() -> Option<Vec<Endpoint>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Direction, default_backend};
    use midi_forge_core::midi2_note_on;

    #[test]
    fn parse_in_and_out_ids() {
        let id = r"\\?\swd#midisrv#midiu_diag_loopback_a#{e7cce071-3c03-423f-88d3-f1045d02552b}";
        let full_in = format!("{IN_PREFIX}{id}");
        let full_out = format!("{OUT_PREFIX}{id}");
        assert_eq!(parse_wms_id(&full_in), Some((WmsDir::Input, id)));
        assert_eq!(parse_wms_id(&full_out), Some((WmsDir::Output, id)));
        assert!(parse_wms_id("winmm:in:0").is_none());
    }

    #[test]
    fn split_ump_words_two_notes() {
        let a = midi2_note_on(0, 1, 60, 0x8000);
        let b = midi2_note_on(0, 1, 64, 0x4000);
        let mut words = Vec::new();
        words.extend_from_slice(a.words());
        words.extend_from_slice(b.words());
        let got = split_ump_words(&words);
        assert_eq!(got, vec![a, b]);
    }

    #[test]
    fn split_ump_words_stops_on_truncated() {
        let a = midi2_note_on(0, 1, 60, 0x8000);
        let mut words = a.words().to_vec();
        words.push(0x4090_3C00); // type 0x4 needs two words
        let got = split_ump_words(&words);
        assert_eq!(got, vec![a]);
    }

    #[test]
    fn wms_init_or_skip() {
        match WmsInit::try_new() {
            Ok(_init) => {}
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Install SDK") && msg.contains("WindowsMIDIServicesSDK"),
                    "{msg}"
                );
            }
        }
    }

    #[test]
    fn wms_backend_try_new_never_fakes_a_session() {
        match WmsBackend::try_new() {
            Ok(backend) => {
                assert_eq!(backend.name(), "wms");
                assert!(backend.caps().native_ump);
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Install SDK")
                        || msg.contains("MidiSession")
                        || msg.contains("winmd")
                        || msg.contains("MIDI Services"),
                    "must explain the missing SDK: {msg}"
                );
            }
        }
    }

    #[test]
    fn default_backend_matches_session_availability() {
        let b = default_backend();
        match WmsBackend::try_new() {
            Ok(_) => {
                assert_eq!(b.name(), "wms");
                assert!(b.caps().native_ump);
            }
            Err(_) => {
                assert_ne!(b.name(), "wms");
                assert!(!b.caps().native_ump);
            }
        }
    }

    #[test]
    fn initializer_clsid_and_iid() {
        assert_eq!(
            CLSID_MIDI_DESKTOP_APP_SDK_INITIALIZER,
            GUID::from_u128(0xc3263827_c3b0_bdbd_2500_ce63a3f3f2c3)
        );
        assert_eq!(
            IID_I_MIDI_CLIENT_INITIALIZER,
            GUID::from_u128(0x8087b303_d551_bce2_1ead_a2500d50c580)
        );
    }

    #[test]
    #[ignore = "needs App SDK runtime, MidiSrv, and a loopback endpoint"]
    fn wms_roundtrip_loopback() {
        let mut backend = WmsBackend::try_new().expect("WmsBackend::try_new");
        backend.refresh().expect("refresh");
        let out = backend
            .endpoints()
            .iter()
            .find(|e| e.direction == Direction::Output && e.id.0.contains("loopback"))
            .cloned()
            .expect("diagnostic or created loopback output");
        let inp = backend
            .endpoints()
            .iter()
            .find(|e| e.direction == Direction::Input && e.id.0.contains("loopback"))
            .cloned()
            .expect("diagnostic or created loopback input");
        backend.open_input(&inp.id, PortId(1)).expect("open input");
        backend
            .open_output(&out.id, PortId(2))
            .expect("open output");
        let note = midi2_note_on(0, 1, 60, 0x8000);
        backend.send(&out.id, &note).expect("send");
        std::thread::sleep(std::time::Duration::from_millis(80));
        let mut got = Vec::new();
        let _ = backend.poll(&mut got);
        assert!(
            got.iter().any(|ev| ev.packet.message_type() == 0x4),
            "expected MIDI 2 note on loopback, got {got:?}"
        );
    }
}
