//! WinMM enumeration. Opening streams is Phase 1.

use crate::backend::{Direction, Endpoint, EndpointId, MidiBackend, ProtocolHint};
use crate::error::IoError;

const MAXPNAMELEN: usize = 32;
const MMSYSERR_NOERROR: u32 = 0;

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

#[link(name = "winmm")]
unsafe extern "system" {
    fn midiInGetNumDevs() -> u32;
    fn midiInGetDevCapsW(device_id: usize, caps: *mut MidiInCapsW, size: u32) -> u32;
    fn midiOutGetNumDevs() -> u32;
    fn midiOutGetDevCapsW(device_id: usize, caps: *mut MidiOutCapsW, size: u32) -> u32;
}

pub struct WinMmBackend {
    endpoints: Vec<Endpoint>,
}

impl WinMmBackend {
    pub fn new() -> Self {
        let mut this = Self {
            endpoints: Vec::new(),
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

impl MidiBackend for WinMmBackend {
    fn name(&self) -> &'static str {
        "winmm"
    }

    fn refresh(&mut self) -> Result<(), IoError> {
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
            let rc = unsafe {
                midiInGetDevCapsW(i as usize, &mut caps, size_of::<MidiInCapsW>() as u32)
            };
            if rc != MMSYSERR_NOERROR {
                return Err(IoError::Backend(format!(
                    "midiInGetDevCapsW({i}) failed: {rc}"
                )));
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
            let rc = unsafe {
                midiOutGetDevCapsW(i as usize, &mut caps, size_of::<MidiOutCapsW>() as u32)
            };
            if rc != MMSYSERR_NOERROR {
                return Err(IoError::Backend(format!(
                    "midiOutGetDevCapsW({i}) failed: {rc}"
                )));
            }
            endpoints.push(Endpoint {
                id: EndpointId(format!("winmm:out:{i}")),
                name: utf16_name(&caps.name),
                direction: Direction::Output,
                protocol: ProtocolHint::Midi1Bytes,
            });
        }

        self.endpoints = endpoints;
        Ok(())
    }

    fn endpoints(&self) -> &[Endpoint] {
        &self.endpoints
    }
}

fn utf16_name(raw: &[u16]) -> String {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}
