//! Audio capture: the `AudioSource` trait and its implementations (plan steps 7.2, 7.3).
//!
//! Samples stay in memory; nothing here writes audio anywhere.

pub mod capture;
pub mod fake;
pub mod resample;

use std::time::Duration;

use crate::AssistError;

/// A stream of 16 kHz mono samples.
pub trait AudioSource: Send {
    /// The next block, waiting at most `timeout`; `Ok(None)` when nothing came. A device
    /// problem is an `Audio` error the caller may ride out; `SourceEnded` is final.
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<f32>>, AssistError>;

    /// Paused means not capturing at all, so the recording indicator can go.
    fn set_paused(&mut self, paused: bool);
}
