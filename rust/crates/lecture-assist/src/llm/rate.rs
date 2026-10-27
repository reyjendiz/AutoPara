//! A requests-per-minute guard for chat calls.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);

/// At most `limit` calls in any 60 seconds.
pub struct RateGuard {
    limit: usize,
    sent: VecDeque<Instant>,
}

impl RateGuard {
    pub fn new(limit: u32) -> Self {
        Self {
            limit: limit as usize,
            sent: VecDeque::new(),
        }
    }

    /// Count a call at `now` if the limit allows it.
    pub fn try_acquire(&mut self, now: Instant) -> bool {
        while self
            .sent
            .front()
            .is_some_and(|&t| now.duration_since(t) >= WINDOW)
        {
            self.sent.pop_front();
        }
        if self.sent.len() >= self.limit {
            return false;
        }
        self.sent.push_back(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_holds_for_a_minute() {
        let start = Instant::now();
        let mut guard = RateGuard::new(2);
        assert!(guard.try_acquire(start));
        assert!(guard.try_acquire(start + Duration::from_secs(10)));
        assert!(!guard.try_acquire(start + Duration::from_secs(59)));
        // The first call has left the window.
        assert!(guard.try_acquire(start + Duration::from_secs(60)));
        assert!(!guard.try_acquire(start + Duration::from_secs(61)));
        assert!(guard.try_acquire(start + Duration::from_secs(70)));
    }

    #[test]
    fn zero_allows_nothing() {
        assert!(!RateGuard::new(0).try_acquire(Instant::now()));
    }
}
