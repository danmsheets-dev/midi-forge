/// Common controller names (GM / MMA). Unknown numbers return `None`.
pub fn cc_name(controller: u8) -> Option<&'static str> {
    Some(match controller {
        0 => "Bank MSB",
        1 => "Mod",
        2 => "Breath",
        4 => "Foot",
        5 => "Portamento time",
        6 => "Data MSB",
        7 => "Volume",
        8 => "Balance",
        10 => "Pan",
        11 => "Expression",
        12 => "Effect 1",
        13 => "Effect 2",
        32 => "Bank LSB",
        33 => "Mod LSB",
        38 => "Data LSB",
        64 => "Sustain",
        65 => "Portamento",
        66 => "Sostenuto",
        67 => "Soft",
        68 => "Legato",
        69 => "Hold 2",
        70 => "Sound variation",
        71 => "Timbre/Res",
        72 => "Release",
        73 => "Attack",
        74 => "Brightness",
        75 => "Decay",
        76 => "Vibrato rate",
        77 => "Vibrato depth",
        78 => "Vibrato delay",
        84 => "Portamento ctrl",
        91 => "Reverb",
        92 => "Tremolo",
        93 => "Chorus",
        94 => "Detune",
        95 => "Phaser",
        96 => "Data inc",
        97 => "Data dec",
        98 => "NRPN LSB",
        99 => "NRPN MSB",
        100 => "RPN LSB",
        101 => "RPN MSB",
        120 => "All sound off",
        121 => "Reset",
        122 => "Local",
        123 => "All notes off",
        124 => "Omni off",
        125 => "Omni on",
        126 => "Mono",
        127 => "Poly",
        _ => return None,
    })
}

pub fn cc_label(controller: u8) -> String {
    match cc_name(controller) {
        Some(n) => format!("CC{controller} ({n})"),
        None => format!("CC{controller}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown() {
        assert_eq!(cc_name(1), Some("Mod"));
        assert_eq!(cc_name(74), Some("Brightness"));
        assert_eq!(cc_name(20), None);
        assert!(cc_label(7).contains("Volume"));
    }
}
