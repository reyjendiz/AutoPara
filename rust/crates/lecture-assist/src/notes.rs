//! The rolling transcript and the notes built from it (plan step 7.7 a–d, f).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Live notes, grouped as the plan asks (7.7 b, d).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notes {
    pub topic: String,
    pub key_points: Vec<String>,
    pub definitions: Vec<String>,
    pub deadlines: Vec<String>,
    /// "Питання / Завдання": questions and tasks heard in class, never answered.
    pub flagged: Vec<String>,
}
