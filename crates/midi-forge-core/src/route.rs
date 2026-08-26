use std::collections::VecDeque;

use crate::event::{PortId, Timestamp};
use crate::ump::UmpMessage;

/// One incoming packet and the thru destinations it actually hit.
#[derive(Clone, Debug)]
pub struct RouteEvent {
    pub time: Timestamp,
    pub from: PortId,
    pub dests: Vec<PortId>,
    pub packet: UmpMessage,
}

#[derive(Clone, Debug)]
pub struct RouteLog {
    events: VecDeque<RouteEvent>,
    cap: usize,
}

impl Default for RouteLog {
    fn default() -> Self {
        Self::new(48)
    }
}

impl RouteLog {
    pub fn new(cap: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(cap.max(1)),
            cap: cap.max(1),
        }
    }

    pub fn push(&mut self, event: RouteEvent) {
        if self.events.len() >= self.cap {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, RouteEvent> {
        self.events.iter()
    }

    pub fn last(&self) -> Option<&RouteEvent> {
        self.events.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ump::UmpMessage;

    #[test]
    fn evicts_oldest() {
        let mut log = RouteLog::new(2);
        let pkt = UmpMessage::midi1_channel_voice(0, 0x90, 60, 1);
        log.push(RouteEvent {
            time: Timestamp::from_nanos(1),
            from: PortId(1),
            dests: vec![PortId(2)],
            packet: pkt,
        });
        log.push(RouteEvent {
            time: Timestamp::from_nanos(2),
            from: PortId(1),
            dests: vec![PortId(2), PortId(3)],
            packet: pkt,
        });
        log.push(RouteEvent {
            time: Timestamp::from_nanos(3),
            from: PortId(1),
            dests: vec![],
            packet: pkt,
        });
        assert_eq!(log.len(), 2);
        assert_eq!(log.iter().next().unwrap().time.nanos, 2);
        assert_eq!(log.last().unwrap().dests.len(), 0);
    }
}
