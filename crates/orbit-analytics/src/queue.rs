//! Bounded in-memory event queue.
//!
//! The queue never grows without limit: once it is full the oldest events are
//! dropped, so analytics can never exhaust memory or disk.

use std::collections::VecDeque;

use crate::event::QueuedEvent;

#[derive(Debug, Clone)]
pub struct EventQueue {
    limit: usize,
    events: VecDeque<QueuedEvent>,
}

impl EventQueue {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            events: VecDeque::new(),
        }
    }

    /// Enqueue an event, dropping the oldest when the cap is reached.
    pub fn push(&mut self, event: QueuedEvent) {
        if self.events.len() >= self.limit {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// Remove and return up to `max` of the oldest events.
    pub fn drain_batch(&mut self, max: usize) -> Vec<QueuedEvent> {
        let take = max.min(self.events.len());
        self.events.drain(..take).collect()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::AnalyticsEvent;

    fn event(n: i64) -> QueuedEvent {
        QueuedEvent::new(AnalyticsEvent::AppStarted, n)
    }

    #[test]
    fn enqueue_and_drain() {
        let mut queue = EventQueue::new(10);
        for n in 0..5 {
            queue.push(event(n));
        }
        assert_eq!(queue.len(), 5);
        let batch = queue.drain_batch(3);
        assert_eq!(batch.len(), 3);
        assert_eq!(batch[0].timestamp_ms, 0);
        assert_eq!(batch[2].timestamp_ms, 2);
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn queue_limit_drops_oldest() {
        let mut queue = EventQueue::new(3);
        for n in 0..5 {
            queue.push(event(n));
        }
        assert_eq!(queue.len(), 3);
        let batch = queue.drain_batch(10);
        // The three newest survive.
        assert_eq!(
            batch.iter().map(|e| e.timestamp_ms).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
    }

    #[test]
    fn clear_empties_the_queue() {
        let mut queue = EventQueue::new(3);
        queue.push(event(1));
        queue.clear();
        assert!(queue.is_empty());
    }
}
