use crate::event::{MidiEvent, PortId};
use crate::filter::Filter;
use crate::map::DataMap;

/// One thru connection: source port → destination port, with a filter and map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub from: PortId,
    pub to: PortId,
    pub filter: Filter,
    pub map: DataMap,
}

/// N×M MIDI thru graph.
#[derive(Clone, Debug, Default)]
pub struct Router {
    links: Vec<Link>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn links(&self) -> &[Link] {
        &self.links
    }

    pub fn is_linked(&self, from: PortId, to: PortId) -> bool {
        self.links.iter().any(|l| l.from == from && l.to == to)
    }

    pub fn filter(&self, from: PortId, to: PortId) -> Option<&Filter> {
        self.links
            .iter()
            .find(|l| l.from == from && l.to == to)
            .map(|l| &l.filter)
    }

    pub fn set_linked(&mut self, from: PortId, to: PortId, linked: bool) {
        if from == to {
            return;
        }
        let existing = self.links.iter().position(|l| l.from == from && l.to == to);
        match (linked, existing) {
            (true, None) => self.links.push(Link {
                from,
                to,
                filter: Filter::default(),
                map: DataMap::default(),
            }),
            (false, Some(i)) => {
                self.links.swap_remove(i);
            }
            _ => {}
        }
    }

    pub fn set_filter(&mut self, from: PortId, to: PortId, filter: Filter) {
        if let Some(link) = self.links.iter_mut().find(|l| l.from == from && l.to == to) {
            link.filter = filter;
        }
    }

    pub fn map(&self, from: PortId, to: PortId) -> Option<&DataMap> {
        self.links
            .iter()
            .find(|l| l.from == from && l.to == to)
            .map(|l| &l.map)
    }

    pub fn set_map(&mut self, from: PortId, to: PortId, map: DataMap) {
        if let Some(link) = self.links.iter_mut().find(|l| l.from == from && l.to == to) {
            link.map = map;
        }
    }

    pub fn clear(&mut self) {
        self.links.clear();
    }

    /// Apply matching filters and maps; emit one event per destination port.
    pub fn route(&self, incoming: &MidiEvent) -> Vec<MidiEvent> {
        let mut out = Vec::new();
        for link in &self.links {
            if link.from != incoming.port {
                continue;
            }
            let Some(packet) = link.filter.apply(&incoming.packet) else {
                continue;
            };
            let Some(packet) = link.map.apply(&packet) else {
                continue;
            };
            out.push(MidiEvent::new(incoming.time, link.to, packet));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Timestamp;
    use crate::ump::UmpMessage;

    fn ev(port: u32, packet: UmpMessage) -> MidiEvent {
        MidiEvent::new(Timestamp::from_nanos(1), PortId(port), packet)
    }

    fn note() -> UmpMessage {
        UmpMessage::midi1_channel_voice(0, 0x90, 60, 127)
    }

    fn clock() -> UmpMessage {
        UmpMessage::midi1_system(0, 0xF8, 0, 0)
    }

    #[test]
    fn fans_out_to_two_outputs() {
        let mut router = Router::new();
        router.set_linked(PortId(1), PortId(10), true);
        router.set_linked(PortId(1), PortId(11), true);
        let routed = router.route(&ev(1, note()));
        let dests: Vec<_> = routed.iter().map(|e| e.port.0).collect();
        assert_eq!(dests, vec![10, 11]);
        assert!(routed.iter().all(|e| e.packet == note()));
    }

    #[test]
    fn missing_link_emits_nothing() {
        let router = Router::new();
        assert!(router.route(&ev(1, note())).is_empty());
    }

    #[test]
    fn unlinking_stops_thru() {
        let mut router = Router::new();
        router.set_linked(PortId(1), PortId(10), true);
        router.set_linked(PortId(1), PortId(10), false);
        assert!(router.route(&ev(1, note())).is_empty());
    }

    #[test]
    fn filter_on_one_link_does_not_affect_the_other() {
        let mut router = Router::new();
        router.set_linked(PortId(1), PortId(10), true);
        router.set_linked(PortId(1), PortId(11), true);
        router.set_filter(
            PortId(1),
            PortId(10),
            Filter {
                clock: false,
                ..Filter::default()
            },
        );
        let routed = router.route(&ev(1, clock()));
        assert_eq!(routed.len(), 1);
        assert_eq!(routed[0].port, PortId(11));
    }

    #[test]
    fn refuses_self_link() {
        let mut router = Router::new();
        router.set_linked(PortId(1), PortId(1), true);
        assert!(!router.is_linked(PortId(1), PortId(1)));
    }

    #[test]
    fn map_runs_after_filter() {
        let mut router = Router::new();
        router.set_linked(PortId(1), PortId(10), true);
        router.set_map(PortId(1), PortId(10), crate::map::DataMap::transpose(12));
        let routed = router.route(&ev(1, note()));
        assert_eq!(routed.len(), 1);
        assert_eq!(routed[0].packet.data1(), 72);
    }
}
