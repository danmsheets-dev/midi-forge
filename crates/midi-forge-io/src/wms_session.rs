//! Windows MIDI Services App SDK bootstrap + `MidiSession` backend.
//!
//! `WmsInit` is a real COM initializer (`CoInitializeEx` MTA +
//! `MidiDesktopAppSdkInitializer`). There is no vendored winmd, so this crate
//! does **not** bind `MidiSession` send/receive. [`WmsBackend::try_new`] always
//! returns `Err` and never claims `native_ump`. WinMM remains `default_backend`
//! until a generated projection can put UMP on a live connection.
//!
//! Current `IMidiClientInitializer` (IID `8087b303-d551-bce2-1ead-a2500d50c580`)
//! vtable after `IUnknown`: `GetInstalledWindowsMidiServicesSdkVersion`, then
//! `EnsureServiceAvailable`. There is no separate `InitializeSdkRuntime` slot;
//! C++/C# wrappers treat the version query as runtime init.

#![cfg(windows)]

use std::ffi::c_void;
use std::ptr;

use midi_forge_core::{MidiEvent, PortId, UmpMessage};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{GUID, HRESULT, IUnknown, Interface};

use crate::backend::{BackendCaps, Endpoint, EndpointId, MidiBackend};
use crate::error::IoError;

const SDK_INSTALL: &str = "Install SDK: winget install Microsoft.WindowsMIDIServicesSDK";

const CLSID_MIDI_DESKTOP_APP_SDK_INITIALIZER: GUID =
    GUID::from_u128(0xc3263827_c3b0_bdbd_2500_ce63a3f3f2c3);
const IID_I_MIDI_CLIENT_INITIALIZER: GUID =
    GUID::from_u128(0x8087b303_d551_bce2_1ead_a2500d50c580);

fn sdk_err() -> IoError {
    IoError::Backend(SDK_INSTALL.into())
}

/// Owns MTA COM and the App SDK initializer. Drop uninitializes COM.
pub struct WmsInit {
    _initializer: IUnknown,
}

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

/// Native UMP backend. Not constructed until a winmd-backed `MidiSession` can
/// send UMP words. `try_new` never returns `Ok` in this build.
pub struct WmsBackend {
    _priv: (),
}

impl WmsBackend {
    pub fn try_new() -> Result<Self, IoError> {
        match WmsInit::try_new() {
            Ok(_init) => Err(IoError::Backend(format!(
                "MidiSession projection not bound (no SDK winmd). {SDK_INSTALL}"
            ))),
            Err(err) => Err(err),
        }
    }
}

impl MidiBackend for WmsBackend {
    fn name(&self) -> &'static str {
        "wms"
    }

    fn refresh(&mut self) -> Result<(), IoError> {
        Err(sdk_err())
    }

    fn endpoints(&self) -> &[Endpoint] {
        &[]
    }

    fn open_input(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        Err(IoError::NotFound(id.0.clone()))
    }

    fn close_input(&mut self, _id: &EndpointId) -> Result<(), IoError> {
        Ok(())
    }

    fn open_output(&mut self, id: &EndpointId, _port: PortId) -> Result<(), IoError> {
        Err(IoError::NotFound(id.0.clone()))
    }

    fn close_output(&mut self, _id: &EndpointId) -> Result<(), IoError> {
        Ok(())
    }

    fn poll(&mut self, _out: &mut Vec<MidiEvent>) -> u64 {
        0
    }

    fn send(&mut self, _id: &EndpointId, _packet: &UmpMessage) -> Result<(), IoError> {
        Err(sdk_err())
    }

    fn send_sysex(&mut self, _id: &EndpointId, _bytes: &[u8]) -> Result<(), IoError> {
        Err(sdk_err())
    }

    fn create_loopback(&mut self, _name: &str) -> Result<(EndpointId, EndpointId), IoError> {
        Err(sdk_err())
    }

    fn remove_loopback(&mut self, id: &EndpointId) -> Result<(), IoError> {
        Err(IoError::NotFound(id.0.clone()))
    }

    fn caps(&self) -> BackendCaps {
        BackendCaps::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::default_backend;

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
                panic!(
                    "WmsBackend must not construct without a live MidiSession send path; got {}",
                    backend.name()
                );
            }
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Install SDK")
                        || msg.contains("MidiSession")
                        || msg.contains("winmd"),
                    "stub path must explain the missing SDK: {msg}"
                );
            }
        }
    }

    #[test]
    fn default_backend_is_not_wms_without_session() {
        let b = default_backend();
        assert_ne!(b.name(), "wms");
        assert!(!b.caps().native_ump);
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
    #[ignore = "needs App SDK runtime, MidiSrv, winmd projection, and a loopback endpoint"]
    fn wms_roundtrip_loopback() {
        panic!("MidiSession send/receive is not bound in this build (no SDK winmd)");
    }
}
