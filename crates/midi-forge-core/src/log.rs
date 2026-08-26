use std::collections::VecDeque;

use crate::event::MidiEvent;

/// Bounded ring of captured events. Oldest entries are dropped when full.
pub struct MonitorLog {
    events: VecDeque<MidiEvent>,
    capacity: usize,
    evicted: u64,
}

impl MonitorLog {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            evicted: 0,
        }
    }

    pub fn push(&mut self, event: MidiEvent) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
            self.evicted += 1;
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

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn evicted(&self) -> u64 {
        self.evicted
    }

    pub fn get(&self, index: usize) -> Option<&MidiEvent> {
        self.events.get(index)
    }
}

impl Default for MonitorLog {
    fn default() -> Self {
        Self::new(10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{PortId, Timestamp};
    use crate::ump::UmpMessage;

    fn ev(n: u64) -> MidiEvent {
        MidiEvent::new(
            Timestamp::from_nanos(n),
            PortId(0),
            UmpMessage::midi1_channel_voice(0, 0x90, n as u8, 1),
        )
    }

    #[test]
    fn evicts_oldest_when_full() {
        let mut log = MonitorLog::new(2);
        log.push(ev(1));
        log.push(ev(2));
        log.push(ev(3));
        assert_eq!(log.len(), 2);
        assert_eq!(log.evicted(), 1);
        assert_eq!(log.get(0).unwrap().time.nanos, 2);
        assert_eq!(log.get(1).unwrap().time.nanos, 3);
    }

    #[test]
    fn clear_keeps_capacity_and_evicted_count() {
        let mut log = MonitorLog::new(2);
        log.push(ev(1));
        log.push(ev(2));
        log.push(ev(3));
        log.clear();
        assert!(log.is_empty());
        assert_eq!(log.evicted(), 1);
    }
}
