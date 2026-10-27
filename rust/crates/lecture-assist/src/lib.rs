//! Lecture Assist: an opt-in study aid that transcribes the class audio locally and keeps notes,
//! shown only on the user's own screen.
//!
//! Text only. Audio lives in memory and is never written to disk; nothing here speaks, joins the
//! meeting, or hides from screen sharing (plan rule R13).

pub mod audio;
pub mod engine;
pub mod helpers;
pub mod llm;
pub mod models;
pub mod notes;
pub mod stt;
pub mod text;
pub mod vad;

pub use engine::{AssistEngine, Handle, Parts};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Everything downstream of capture works at 16 kHz mono, whisper's input.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum AssistError {
    #[error("audio: {0}")]
    Audio(String),
    #[error("the audio source ended")]
    SourceEnded,
    #[error("model: {0}")]
    Model(String),
    #[error("recognition: {0}")]
    Recognition(String),
    #[error("chat: {0}")]
    Chat(String),
    #[error("the chat provider is busy or unreachable: {0}")]
    ChatUnavailable(String),
    #[error("too many requests this minute")]
    RateLimited,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl AssistError {
    /// The string-table key the interface shows for this error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Audio(_) | Self::SourceEnded => "assist_error_audio",
            Self::Model(_) => "assist_error_model",
            Self::Recognition(_) => "assist_error_recognition",
            Self::Chat(_) | Self::ChatUnavailable(_) => "assist_error_chat",
            Self::RateLimited => "assist_error_rate",
            Self::Io(_) => "assist_error_io",
        }
    }
}

/// The class being listened to, from the scheduler; it shapes the prompts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClassContext {
    pub subject: String,
    pub teacher: String,
    pub group: String,
    /// Local wall time, `YYYY-MM-DDTHH:MM`, when known.
    pub start: Option<String>,
    pub end: Option<String>,
}

/// Recognition language: detected among the three, or fixed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Language {
    #[default]
    Auto,
    Uk,
    Ru,
    En,
}

impl Language {
    pub fn code(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Uk => Some("uk"),
            Self::Ru => Some("ru"),
            Self::En => Some("en"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssistConfig {
    pub language: Language,
    pub glossary: Vec<String>,
    /// Audio time between automatic note updates.
    pub notes_every_secs: u32,
    /// How much recent transcript the notes and helpers see, beside the running summary.
    pub window_minutes: u32,
    pub requests_per_minute: u32,
    /// How often the open utterance is re-read for live text; 0 turns partials off.
    pub partial_every_ms: u32,
}

impl Default for AssistConfig {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            glossary: Vec::new(),
            notes_every_secs: 75,
            window_minutes: 5,
            requests_per_minute: 6,
            partial_every_ms: 1000,
        }
    }
}

/// `Off → Starting → Listening ⇄ Paused → Stopping → Off`, or `Error` (plan step 7.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", content = "code", rename_all = "camelCase")]
#[ts(export, rename = "AssistState")]
pub enum State {
    Off,
    Starting,
    Listening,
    Paused,
    Stopping,
    Error(String),
}

impl State {
    /// Audio is being captured: the indicator must show.
    pub fn capturing(&self) -> bool {
        matches!(self, Self::Starting | Self::Listening)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FlagKind {
    Question,
    Task,
}

/// The on-demand helpers (plan 7.7 e) and the end-of-class summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AskKind {
    /// "Підсумок останніх 2 хв"
    Recap,
    /// "Поясни простіше", for the given text or the last phrase.
    Simplify,
    /// "Коротко про тему"
    Topic,
    /// The user's own question about the class.
    Free,
    /// Written once when listening stops.
    Summary,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Segment {
    /// Milliseconds from the start of listening.
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub flag: Option<FlagKind>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Term {
    pub term: String,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum AssistEvent {
    StateChanged {
        state: State,
    },
    /// Input loudness, 0–1, about ten times a second.
    Level {
        level: f32,
    },
    TranscriptPartial {
        text: String,
    },
    TranscriptFinal {
        segment: Segment,
    },
    NotesUpdated {
        notes: notes::Notes,
    },
    TermsUpdated {
        terms: Vec<Term>,
    },
    Flagged {
        segment: Segment,
    },
    /// A helper's reply as it streams; `done` marks the last piece.
    HelperReply {
        kind: AskKind,
        text: String,
        done: bool,
    },
    Error {
        code: String,
        detail: String,
    },
}
