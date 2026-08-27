//! Generate Windows MIDI Services WinRT bindings from a vendored winmd.
//! Runtime still requires the user's installed App SDK (never vendor the DLL).

fn main() {
    println!("cargo:rustc-check-cfg=cfg(midi2_has_winmd)");
    println!("cargo:rerun-if-env-changed=MIDI2_WINMD");
    println!("cargo:rerun-if-changed=../../vendor/Windows.Devices.Midi2.winmd");
    println!("cargo:rerun-if-changed=../../vendor/Microsoft.Windows.Devices.Midi2.winmd");

    let winmd = std::env::var_os("MIDI2_WINMD")
        .map(std::path::PathBuf::from)
        .or_else(find_vendor_winmd);

    #[cfg(windows)]
    if let Some(winmd) = winmd {
        generate_winrt(&winmd);
        println!("cargo:rustc-cfg=midi2_has_winmd");
    }
}

fn find_vendor_winmd() -> Option<std::path::PathBuf> {
    let roots = [
        std::path::Path::new("../../vendor"),
        std::path::Path::new("vendor"),
    ];
    for root in roots {
        for name in [
            "Windows.Devices.Midi2.winmd",
            "Microsoft.Windows.Devices.Midi2.winmd",
        ] {
            let p = root.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(windows)]
fn generate_winrt(winmd: &std::path::Path) {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("wms_winrt.rs");
    let winmd = winmd.to_string_lossy();
    let out_s = out.to_string_lossy();
    let _ = windows_bindgen::bindgen([
        "--in",
        "default",
        "--in",
        winmd.as_ref(),
        "--out",
        out_s.as_ref(),
        "--filter",
        "Windows.Devices.Midi2",
        "!Windows.Devices.Midi2.CapabilityInquiry",
        "!Windows.Devices.Midi2.ClientPlugins",
        "!Windows.Devices.Midi2.Reporting",
        "!Windows.Devices.Midi2.ServiceConfig",
        "!Windows.Devices.Midi2.Transports",
        "!Windows.Devices.Midi2.Utilities",
    ]);
    patch_generated_winrt(out.as_path());
}

/// bindgen 0.62 emits `#![allow]` (illegal in include!) and factory closures
/// that return `R` while `FactoryCache::call` wants `Result<R>`. MIDI APIs are
/// `[noexcept]`, so adapt the generated factory helpers.
fn patch_generated_winrt(path: &std::path::Path) {
    let src = std::fs::read_to_string(path).expect("read generated wms_winrt.rs");
    let src = src.replacen("#![allow(", "#[allow(", 1);
    let src = src.replace("windows_core::Result<R>", "R");
    let src = src.replace(
        "SHARED.call(callback)",
        "SHARED.call(|this| Ok(callback(this))).expect(\"Windows.Devices.Midi2 factory\")",
    );
    std::fs::write(path, src).expect("write patched wms_winrt.rs");
}
