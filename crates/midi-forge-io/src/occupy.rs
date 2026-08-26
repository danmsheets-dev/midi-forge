use crate::error::IoError;
use crate::wms::probe_wms;

const DAW_HINTS: &[&str] = &[
    "ableton",
    "fl studio",
    "cubase",
    "nuendo",
    "reaper",
    "studio one",
    "bitwig",
    "pro tools",
    "cakewalk",
    "reason",
    "midi-ox",
    "midiox",
    "bome",
    "protokol",
    "showmidi",
    "kontakt",
    "gig performer",
    "mainstage",
    "logic pro",
    "max 8",
    "max 9",
    "max/msp",
];

/// Visible windows that look like MIDI apps. Heuristic — WinMM cannot name the holder.
pub fn likely_midi_holders() -> Vec<String> {
    #[cfg(windows)]
    {
        windows_titles()
            .into_iter()
            .filter(|t| {
                let l = t.to_ascii_lowercase();
                if l.contains("midi-forge") {
                    return false;
                }
                DAW_HINTS.iter().any(|h| l.contains(h))
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Technician-facing exclusive-open copy. Other errors pass through.
pub fn explain_in_use(err: &IoError, device: &str) -> String {
    match err {
        IoError::InUse(_) => {
            let holders = likely_midi_holders();
            let who = if holders.is_empty() {
                "another application".to_string()
            } else {
                holders.join(", ")
            };
            if probe_wms().midisrv {
                format!(
                    "{device} is exclusive-open ({who}). MidiSrv is running — close that app and Refresh."
                )
            } else {
                format!(
                    "{device} is exclusive-open ({who}). WinMM is one-app-only. Close that app, or enable Windows MIDI Services for multi-client."
                )
            }
        }
        other => other.to_string(),
    }
}

#[cfg(windows)]
fn windows_titles() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let _ = EnumWindows(enum_proc, std::ptr::from_mut(&mut out) as isize);
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(windows)]
unsafe extern "system" fn enum_proc(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
    unsafe {
        let out = &mut *(lparam as *mut Vec<String>);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return 1;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n > 0 {
            let title = String::from_utf16_lossy(&buf[..n as usize]);
            if !title.is_empty() {
                out.push(title);
            }
        }
        1
    }
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(
        cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
        lparam: isize,
    ) -> i32;
    fn GetWindowTextW(hwnd: *mut core::ffi::c_void, lp: *mut u16, n: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: *mut core::ffi::c_void) -> i32;
    fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_inuse_passthrough() {
        let err = IoError::NotFound("winmm:in:0".into());
        assert!(explain_in_use(&err, "MPK").contains("not found"));
    }

    #[test]
    fn inuse_mentions_device_and_winmm() {
        let err = IoError::InUse("winmm:in:0".into());
        let s = explain_in_use(&err, "MPK mini play");
        assert!(s.contains("MPK mini play"));
        assert!(s.contains("exclusive-open"));
    }

    #[test]
    fn holders_does_not_panic() {
        let _ = likely_midi_holders();
    }
}
