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
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    for dir in path.split(';') {
        let p = std::path::Path::new(dir).join("midi.exe");
        if p.is_file() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_summary() {
        let s = probe_wms();
        assert!(!s.summary.is_empty());
    }
}
