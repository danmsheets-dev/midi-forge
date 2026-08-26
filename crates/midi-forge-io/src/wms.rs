//! Windows MIDI Services diagnostics. Native MidiSession I/O needs the App SDK
//! projection (WinRT / NuGet), which is not a crates.io crate.

#[cfg(windows)]
use crate::winmm::midisrv_running;

#[derive(Clone, Debug)]
pub struct WmsStatus {
    pub midisrv: bool,
    pub midi_cli: bool,
    pub summary: String,
}

pub fn probe_wms() -> WmsStatus {
    #[cfg(windows)]
    {
        let midisrv = midisrv_running();
        let midi_cli = std::env::var_os("WINDIR")
            .map(|w| {
                let p = std::path::Path::new(&w).join("System32").join("midi.exe");
                p.is_file()
            })
            .unwrap_or(false)
            || which_midi();
        let summary = match (midisrv, midi_cli) {
            (true, true) => {
                "MidiSrv + midi.exe. WinMM is multi-client via the service. Native UMP MidiSession still needs the App SDK runtime."
                    .into()
            }
            (true, false) => {
                "MidiSrv running. Install SDK Tools (winget Microsoft.WindowsMIDIServicesSDK) for loopback UI and midi.exe."
                    .into()
            }
            (false, _) => {
                "MidiSrv not running. WinMM is exclusive-open. Enable Windows MIDI Services (Win11) or use loopMIDI for DAW-visible cables."
                    .into()
            }
        };
        WmsStatus {
            midisrv,
            midi_cli,
            summary,
        }
    }
    #[cfg(not(windows))]
    {
        WmsStatus {
            midisrv: false,
            midi_cli: false,
            summary: "Windows MIDI Services is Windows-only. macOS uses CoreMIDI virtual ports."
                .into(),
        }
    }
}

#[cfg(windows)]
fn which_midi() -> bool {
    midi_cli_path().is_some()
}

pub fn midi_cli_path() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        if let Some(w) = std::env::var_os("WINDIR") {
            let p = std::path::Path::new(&w).join("System32").join("midi.exe");
            if p.is_file() {
                return Some(p);
            }
        }
        let Ok(path) = std::env::var("PATH") else {
            return None;
        };
        for dir in path.split(';') {
            let p = std::path::Path::new(dir).join("midi.exe");
            if p.is_file() {
                return Some(p);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// DAW-visible MIDI 2 loopback pair via `midi loopback create --root-name`.
/// Temporary: lives until that console/service session ends. Not a MidiSession.
pub fn create_wms_loopback(root_name: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let name = root_name.trim();
        if name.is_empty() {
            return Err("Need a loopback name".into());
        }
        let exe = midi_cli_path().ok_or_else(|| {
            "midi.exe not found. Install SDK Tools: winget install Microsoft.WindowsMIDIServicesSDK"
                .to_string()
        })?;
        if !probe_wms().midisrv {
            return Err("MidiSrv is not running. Enable Windows MIDI Services, then retry.".into());
        }
        let out = std::process::Command::new(&exe)
            .args(["loopback", "create", "--root-name", name])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("midi.exe: {e}"))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            let msg = format!("{} {}", stderr.trim(), stdout.trim());
            return Err(format!("midi loopback create failed: {}", msg.trim()));
        }
        if stdout.trim().is_empty() {
            Ok(format!(
                "Created WMS loopback '{name}' (A/B). DAWs should see it after Refresh."
            ))
        } else {
            Ok(stdout.trim().to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = root_name;
        Err("WMS loopbacks are Windows-only. macOS: Add cable (CoreMIDI).".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_summary() {
        let s = probe_wms();
        assert!(!s.summary.is_empty());
    }

    #[test]
    fn empty_name_rejected() {
        assert!(create_wms_loopback("  ").is_err());
    }
}
