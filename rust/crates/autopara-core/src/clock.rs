//! Time, as the scheduler sees it (plan step 3.1). Nothing in the scheduler reads the system time
//! directly: it asks a [`Clock`], so tests can move time however they like.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime};

/// Two readings of "now": the local wall clock, which the timetable is written in and which the
/// user (or a time-zone change) can move, and a monotonic counter, which only ever runs forward.
/// Comparing the two is how a sleep or a clock change is noticed.
pub trait Clock: Send + Sync {
    /// Local wall-clock time, without a time zone, as the timetable is written.
    fn now(&self) -> NaiveDateTime;
    /// Time elapsed since an arbitrary fixed point; never goes backwards.
    fn monotonic(&self) -> Duration;
}

/// The real clock.
#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> NaiveDateTime {
        Local::now().naive_local()
    }

    fn monotonic(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// A clock for tests: both readings are set by hand.
#[derive(Debug)]
pub struct FakeClock {
    state: Mutex<(NaiveDateTime, Duration)>,
}

impl FakeClock {
    pub fn new(now: NaiveDateTime) -> Self {
        Self {
            state: Mutex::new((now, Duration::ZERO)),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut (NaiveDateTime, Duration)) -> T) -> T {
        f(&mut self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// Time passes normally: both readings move by `by`.
    pub fn advance(&self, by: Duration) {
        self.with(|(wall, mono)| {
            *wall += by;
            *mono += by;
        });
    }

    /// The wall clock is set to `to` while no time passes (a user or a time-zone change).
    pub fn set_wall(&self, to: NaiveDateTime) {
        self.with(|(wall, _)| *wall = to);
    }

    /// Wall and monotonic readings set independently (a sleep: both jump, or neither).
    pub fn set(&self, wall: NaiveDateTime, monotonic: Duration) {
        self.with(|state| *state = (wall, monotonic));
    }
}

impl Clock for FakeClock {
    fn now(&self) -> NaiveDateTime {
        self.with(|(wall, _)| *wall)
    }

    fn monotonic(&self) -> Duration {
        self.with(|(_, mono)| *mono)
    }
}
