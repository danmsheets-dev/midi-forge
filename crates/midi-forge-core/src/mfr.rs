/// MMA manufacturer ID → short name for identity replies.
pub fn manufacturer_name(id: &[u8]) -> Option<&'static str> {
    Some(match id {
        [0x01] => "Sequential",
        [0x07] => "Kurzweil",
        [0x0F] => "Fairlight",
        [0x10] => "Fairlight",
        [0x18] => "E-mu",
        [0x1A] => "ART",
        [0x1C] => "Linn",
        [0x21] => "Moog",
        [0x23] => "PPG",
        [0x29] => "PPG",
        [0x2D] => "Hohner",
        [0x33] => "Hammond",
        [0x3E] => "Waldorf",
        [0x40] => "Kawai",
        [0x41] => "Roland",
        [0x42] => "Korg",
        [0x43] => "Yamaha",
        [0x44] => "Casio",
        [0x47] => "Akai",
        [0x4C] => "Sony",
        [0x51] => "Fostex",
        [0x52] => "Zoom",
        [0x5F] => "Ensoniq",
        [0x00, 0x00, 0x0E] => "Alesis",
        [0x00, 0x00, 0x10] => "DOD",
        [0x00, 0x01, 0x05] => "Open Labs",
        [0x00, 0x20, 0x29] => "Focusrite/Novation",
        [0x00, 0x20, 0x32] => "Native Instruments",
        [0x00, 0x20, 0x3C] => "Teenage Engineering",
        [0x00, 0x21, 0x09] => "Pioneer DJ",
        [0x00, 0x21, 0x1D] => "Arturia",
        [0x00, 0x21, 0x27] => "Elektron",
        [0x00, 0x21, 0x4E] => "1010music",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roland_and_extended_alesis() {
        assert_eq!(manufacturer_name(&[0x41]), Some("Roland"));
        assert_eq!(manufacturer_name(&[0x00, 0x00, 0x0E]), Some("Alesis"));
        assert_eq!(manufacturer_name(&[0x7F]), None);
    }
}
