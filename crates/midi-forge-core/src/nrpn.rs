use crate::ump::UmpMessage;

const CC_DATA_MSB: u8 = 6;
const CC_DATA_LSB: u8 = 38;
const CC_NRPN_LSB: u8 = 98;
const CC_NRPN_MSB: u8 = 99;
const CC_RPN_LSB: u8 = 100;
const CC_RPN_MSB: u8 = 101;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParamKind {
    Rpn,
    Nrpn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParamValue {
    pub kind: ParamKind,
    pub channel: u8,
    pub msb: u8,
    pub lsb: u8,
    pub value: u16,
}

impl ParamValue {
    pub fn summary(&self) -> String {
        let kind = match self.kind {
            ParamKind::Rpn => "RPN",
            ParamKind::Nrpn => "NRPN",
        };
        let name = rpn_name(self.kind, self.msb, self.lsb);
        match name {
            Some(n) => format!(
                "Ch{} {kind} {}:{} ({n}) {}",
                self.channel + 1,
                self.msb,
                self.lsb,
                self.value
            ),
            None => format!(
                "Ch{} {kind} {}:{} {}",
                self.channel + 1,
                self.msb,
                self.lsb,
                self.value
            ),
        }
    }
}

pub fn rpn_name(kind: ParamKind, msb: u8, lsb: u8) -> Option<&'static str> {
    if kind != ParamKind::Rpn {
        return None;
    }
    Some(match (msb, lsb) {
        (0, 0) => "Pitch bend range",
        (0, 1) => "Fine tune",
        (0, 2) => "Coarse tune",
        (0, 3) => "Tuning program",
        (0, 4) => "Tuning bank",
        (0, 5) => "Mod depth range",
        (0, 6) => "MPE config",
        (127, 127) => "Null",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct Cell {
    kind: Option<ParamKind>,
    msb: Option<u8>,
    lsb: Option<u8>,
    data_msb: Option<u8>,
}

/// Assembles RPN/NRPN from CC 98–101 + data entry.
#[derive(Clone, Debug)]
pub struct NrpnTracker {
    cells: [Cell; 16],
    last: Option<ParamValue>,
}

impl Default for NrpnTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl NrpnTracker {
    pub fn new() -> Self {
        Self {
            cells: [Cell::default(); 16],
            last: None,
        }
    }

    pub fn last(&self) -> Option<ParamValue> {
        self.last
    }

    pub fn push(&mut self, packet: &UmpMessage) -> Option<ParamValue> {
        let p = if packet.message_type() == 0x4 {
            crate::midi2::downscale_to_midi1(packet)
                .into_iter()
                .find(|x| x.message_type() == 0x2)
        } else {
            Some(*packet)
        };
        let p = p?;
        if p.message_type() != 0x2 || p.status_byte() & 0xF0 != 0xB0 {
            return None;
        }
        let ch = (p.status_byte() & 0x0F) as usize;
        let cc = p.data1();
        let val = p.data2();
        let cell = &mut self.cells[ch];
        match cc {
            CC_RPN_MSB => {
                cell.kind = Some(ParamKind::Rpn);
                cell.msb = Some(val);
            }
            CC_RPN_LSB => {
                cell.kind = Some(ParamKind::Rpn);
                cell.lsb = Some(val);
            }
            CC_NRPN_MSB => {
                cell.kind = Some(ParamKind::Nrpn);
                cell.msb = Some(val);
            }
            CC_NRPN_LSB => {
                cell.kind = Some(ParamKind::Nrpn);
                cell.lsb = Some(val);
            }
            CC_DATA_MSB => {
                cell.data_msb = Some(val);
                return self.commit(ch as u8, val, 0);
            }
            CC_DATA_LSB => {
                let msb = cell.data_msb.unwrap_or(0);
                return self.commit(ch as u8, msb, val);
            }
            _ => {}
        }
        None
    }

    fn commit(&mut self, channel: u8, msb_data: u8, lsb_data: u8) -> Option<ParamValue> {
        let cell = self.cells[usize::from(channel)];
        let kind = cell.kind?;
        let msb = cell.msb?;
        let lsb = cell.lsb?;
        let value = (u16::from(msb_data) << 7) | u16::from(lsb_data);
        let pv = ParamValue {
            kind,
            channel,
            msb,
            lsb,
            value,
        };
        self.last = Some(pv);
        Some(pv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(ch: u8, n: u8, v: u8) -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0xB0 | ch, n, v)
    }

    #[test]
    fn mpe_rpn_assembles() {
        let mut t = NrpnTracker::new();
        assert!(t.push(&cc(0, 101, 0)).is_none());
        assert!(t.push(&cc(0, 100, 6)).is_none());
        let v = t.push(&cc(0, 6, 7)).unwrap();
        assert_eq!(v.kind, ParamKind::Rpn);
        assert_eq!(v.msb, 0);
        assert_eq!(v.lsb, 6);
        assert_eq!(v.value, 7 << 7);
        assert!(v.summary().contains("MPE"));
    }

    #[test]
    fn nrpn_assembles_with_lsb() {
        let mut t = NrpnTracker::new();
        assert!(t.push(&cc(1, 99, 1)).is_none());
        assert!(t.push(&cc(1, 98, 2)).is_none());
        assert!(t.push(&cc(1, 6, 10)).is_some());
        let v = t.push(&cc(1, 38, 5)).unwrap();
        assert_eq!(v.kind, ParamKind::Nrpn);
        assert_eq!(v.channel, 1);
        assert_eq!(v.msb, 1);
        assert_eq!(v.lsb, 2);
        assert_eq!(v.value, (10 << 7) | 5);
        assert!(v.summary().contains("NRPN"));
    }
}
