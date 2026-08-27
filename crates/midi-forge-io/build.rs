//! Optional MIDI 2 winmd hook. Bindings are hand-written COM/WinRT ABI;
//! a present winmd is noted but not required to compile.

fn main() {
    println!("cargo:rerun-if-env-changed=MIDI2_WINMD");
    println!("cargo:rerun-if-changed=vendor/Microsoft.Windows.Devices.Midi2.winmd");

    let env_winmd = std::env::var_os("MIDI2_WINMD");
    let vendor = std::path::Path::new("vendor/Microsoft.Windows.Devices.Midi2.winmd");
    if env_winmd.is_some() || vendor.is_file() {
        // Hand-written ABI is used in this crate. Do not check a native DLL into git.
        println!("cargo:rustc-cfg=midi2_has_winmd");
    }
}
