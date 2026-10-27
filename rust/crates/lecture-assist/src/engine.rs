//! `AssistEngine`: the pipeline from audio to text, on its own thread (plan step 7.1).
//!
//! Capture, voice detection and recognition block, so they run on one dedicated thread; chat
//! requests run as tasks on the caller's tokio runtime. A panic anywhere in the pipeline ends
//! in an `Error` state, never in the caller: the scheduler keeps joining classes regardless.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;

use crate::audio::AudioSource;
use crate::helpers::helper_messages;
use crate::llm::rate::RateGuard;
use crate::llm::{ChatProvider, complete};
use crate::notes::{Notes, Transcript, notes_messages, parse_reply};
use crate::stt::Recognizer;
use crate::vad::{FRAME, Segmenter, SegmenterConfig, Utterance, Vad, rms};
use crate::{
    AskKind, AssistConfig, AssistError, AssistEvent, ClassContext, SAMPLE_RATE, Segment, State,
    Term, text,
};

/// Builds a part on the engine's thread, so a slow model load never blocks the caller.
pub type Loader<T> = Box<dyn FnOnce() -> Result<T, AssistError> + Send>;

pub struct Parts {
    pub source: Loader<Box<dyn AudioSource>>,
    pub recognizer: Loader<Box<dyn Recognizer>>,
    pub vad: Loader<Box<dyn Vad>>,
    /// `None`: transcript and flags only, no notes or helpers.
    pub chat: Option<Arc<dyn ChatProvider>>,
}

enum Command {
    Pause,
    Resume,
    Stop,
    Ask(AskKind, String),
}

/// Controls a running engine. Dropping it stops the engine too.
pub struct Handle {
    commands: Sender<Command>,
}

impl Handle {
    pub fn pause(&self) {
        let _ = self.commands.send(Command::Pause);
    }

    pub fn resume(&self) {
        let _ = self.commands.send(Command::Resume);
    }

    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// Run a helper; its reply streams back as `HelperReply` events.
    pub fn ask(&self, kind: AskKind, text: impl Into<String>) {
        let _ = self.commands.send(Command::Ask(kind, text.into()));
    }
}

pub struct AssistEngine;

impl AssistEngine {
    /// Start listening. Call from inside a tokio runtime: chat requests run on it.
    pub fn start(
        config: AssistConfig,
        context: ClassContext,
        parts: Parts,
    ) -> (Handle, UnboundedReceiver<AssistEvent>) {
        let (events, receiver) = unbounded_channel();
        let (commands, command_rx) = mpsc::channel();
        let runtime = tokio::runtime::Handle::current();
        let crashed = events.clone();
        let spawned = std::thread::Builder::new()
            .name("assist-engine".into())
            .spawn(move || {
                let run = AssertUnwindSafe(|| {
                    Worker::new(config, context, parts.chat, events, runtime).run(
                        parts.source,
                        parts.recognizer,
                        parts.vad,
                        &command_rx,
                    )
                });
                if catch_unwind(run).is_err() {
                    tracing::error!("the assistant crashed");
                    fail(&crashed, "assist_error_crash", "panic");
                }
            });
        if let Err(error) = spawned {
            tracing::error!(%error, "could not start the assistant thread");
        }
        (Handle { commands }, receiver)
    }
}

fn emit(events: &UnboundedSender<AssistEvent>, event: AssistEvent) {
    let _ = events.send(event);
}

fn report(events: &UnboundedSender<AssistEvent>, error: &AssistError) {
    tracing::warn!(%error, "assistant");
    emit(
        events,
        AssistEvent::Error {
            code: error.code().into(),
            detail: error.to_string(),
        },
    );
}

fn fail(events: &UnboundedSender<AssistEvent>, code: &str, detail: &str) {
    emit(
        events,
        AssistEvent::Error {
            code: code.into(),
            detail: detail.into(),
        },
    );
    emit(
        events,
        AssistEvent::StateChanged {
            state: State::Error(code.into()),
        },
    );
}

/// What chat tasks share with the worker.
#[derive(Default)]
struct Shared {
    transcript: Transcript,
    notes: Notes,
    terms: Vec<Term>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Worker {
    config: AssistConfig,
    context: ClassContext,
    chat: Option<Arc<dyn ChatProvider>>,
    events: UnboundedSender<AssistEvent>,
    runtime: tokio::runtime::Handle,
    shared: Arc<Mutex<Shared>>,
    rate: Arc<Mutex<RateGuard>>,
    prompt: String,
    /// The last final text, to strip what a forced cut repeats.
    previous: String,
    previous_forced: bool,
    notes_task: Option<JoinHandle<()>>,
}

impl Worker {
    fn new(
        config: AssistConfig,
        context: ClassContext,
        chat: Option<Arc<dyn ChatProvider>>,
        events: UnboundedSender<AssistEvent>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        let prompt = text::initial_prompt(&context, &config.glossary);
        let rate = Arc::new(Mutex::new(RateGuard::new(config.requests_per_minute)));
        Self {
            config,
            context,
            chat,
            events,
            runtime,
            shared: Arc::default(),
            rate,
            prompt,
            previous: String::new(),
            previous_forced: false,
            notes_task: None,
        }
    }

    fn state(&self, state: State) {
        emit(&self.events, AssistEvent::StateChanged { state });
    }

    fn run(
        mut self,
        source: Loader<Box<dyn AudioSource>>,
        recognizer: Loader<Box<dyn Recognizer>>,
        vad: Loader<Box<dyn Vad>>,
        commands: &Receiver<Command>,
    ) {
        self.state(State::Starting);
        let loaded = (|| Ok::<_, AssistError>((recognizer()?, vad()?, source()?)))();
        let (mut recognizer, mut vad, mut source) = match loaded {
            Ok(parts) => parts,
            Err(error) => {
                tracing::error!(%error, "the assistant could not start");
                fail(&self.events, error.code(), &error.to_string());
                return;
            }
        };
        self.state(State::Listening);

        let mut segmenter = Segmenter::new(SegmenterConfig::default());
        let mut pending: Vec<f32> = Vec::new();
        let mut paused = false;
        let mut last_partial = 0;
        let mut last_level = 0;
        let mut last_notes = 0;
        let mut notes_through_ms = 0;
        let partial_every = u64::from(self.config.partial_every_ms) * u64::from(SAMPLE_RATE) / 1000;
        let notes_every = u64::from(self.config.notes_every_secs) * u64::from(SAMPLE_RATE);

        loop {
            // While paused, wait on commands instead of spinning.
            let command = if paused {
                commands.recv_timeout(Duration::from_millis(250))
            } else {
                commands.try_recv().map_err(|e| match e {
                    mpsc::TryRecvError::Empty => RecvTimeoutError::Timeout,
                    mpsc::TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                })
            };
            match command {
                Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Command::Pause) if !paused => {
                    paused = true;
                    source.set_paused(true);
                    if let Some(open) = segmenter.flush() {
                        self.finalize(recognizer.as_mut(), open);
                    }
                    self.state(State::Paused);
                }
                Ok(Command::Resume) if paused => {
                    paused = false;
                    source.set_paused(false);
                    self.state(State::Listening);
                }
                Ok(Command::Ask(kind, text)) => self.ask(kind, text),
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
            }
            if paused {
                continue;
            }

            let samples = match source.read(Duration::from_millis(100)) {
                Ok(Some(samples)) => samples,
                Ok(None) => continue,
                Err(AssistError::SourceEnded) => break,
                Err(error) => {
                    report(&self.events, &error);
                    continue;
                }
            };
            pending.extend(samples);
            let whole = pending.len() / FRAME * FRAME;
            if whole == 0 {
                continue;
            }
            let block: Vec<f32> = pending.drain(..whole).collect();
            let position = segmenter.position() + block.len() as u64;
            if position - last_level >= u64::from(SAMPLE_RATE) / 10 {
                last_level = position;
                let level = (rms(&block) * 4.0).min(1.0);
                emit(&self.events, AssistEvent::Level { level });
            }

            let probabilities = vad.probabilities(&block);
            for utterance in segmenter.push(&block, &probabilities) {
                self.finalize(recognizer.as_mut(), utterance);
            }
            if partial_every > 0
                && position - last_partial >= partial_every
                && let Some(open) = segmenter.open()
                && open.len() >= SAMPLE_RATE as usize / 2
            {
                last_partial = position;
                self.partial(recognizer.as_mut(), open);
            }
            let heard_ms = lock(&self.shared).transcript.end_ms();
            if position - last_notes >= notes_every && heard_ms > notes_through_ms {
                last_notes = position;
                notes_through_ms = heard_ms;
                self.notes();
            }
        }

        self.state(State::Stopping);
        source.set_paused(true);
        drop(source);
        if let Some(open) = segmenter.flush() {
            self.finalize(recognizer.as_mut(), open);
        }
        self.finish();
    }

    fn partial(&self, recognizer: &mut dyn Recognizer, open: &[f32]) {
        match recognizer.transcribe(&padded(open), &self.prompt) {
            Ok(pieces) => {
                let joined = join(pieces.iter().map(|p| p.text.as_str()));
                let previous = if self.previous_forced {
                    self.previous.as_str()
                } else {
                    ""
                };
                if let Some(text) = text::clean(previous, &joined) {
                    emit(&self.events, AssistEvent::TranscriptPartial { text });
                }
            }
            Err(error) => report(&self.events, &error),
        }
    }

    fn finalize(&mut self, recognizer: &mut dyn Recognizer, utterance: Utterance) {
        let pieces = match recognizer.transcribe(&padded(&utterance.audio), &self.prompt) {
            Ok(pieces) => pieces,
            Err(error) => return report(&self.events, &error),
        };
        let joined = join(pieces.iter().map(|p| p.text.as_str()));
        let previous = if self.previous_forced {
            self.previous.as_str()
        } else {
            ""
        };
        let cleaned = text::clean(previous, &joined);
        self.previous_forced = utterance.forced;
        let Some(text) = cleaned else { return };
        self.previous = text.clone();
        let rate = u64::from(SAMPLE_RATE);
        let start_ms = utterance.start_sample * 1000 / rate;
        let segment = Segment {
            start_ms,
            end_ms: start_ms + utterance.audio.len() as u64 * 1000 / rate,
            flag: text::detect_flag(&text),
            text,
        };
        let notes = {
            let mut shared = lock(&self.shared);
            shared.transcript.push(segment.clone());
            segment.flag.map(|_| {
                shared.notes.flagged.push(segment.text.clone());
                shared.notes.clone()
            })
        };
        emit(
            &self.events,
            AssistEvent::TranscriptFinal {
                segment: segment.clone(),
            },
        );
        if let Some(notes) = notes {
            emit(&self.events, AssistEvent::Flagged { segment });
            emit(&self.events, AssistEvent::NotesUpdated { notes });
        }
    }

    /// Rolling notes; skipped while the previous request is still out or the rate is spent.
    fn notes(&mut self) {
        let Some(chat) = self.chat.clone() else {
            return;
        };
        if self.notes_task.as_ref().is_some_and(|t| !t.is_finished())
            || !lock(&self.rate).try_acquire(Instant::now())
        {
            return;
        }
        let messages = {
            let shared = lock(&self.shared);
            notes_messages(
                &self.context,
                &shared.transcript,
                self.config.window_minutes,
            )
        };
        let (shared, events) = (self.shared.clone(), self.events.clone());
        self.notes_task = Some(self.runtime.spawn(async move {
            let reply = match complete(chat.as_ref(), messages).await {
                Ok(reply) => reply,
                Err(error) => return report(&events, &error),
            };
            let Some(reply) = parse_reply(&reply) else {
                let error = AssistError::Chat("the notes reply was not JSON".into());
                return report(&events, &error);
            };
            let (notes, terms) = {
                let mut shared = lock(&shared);
                shared.notes.apply(&reply);
                if !reply.summary.trim().is_empty() {
                    shared.transcript.summary = reply.summary.clone();
                }
                for term in reply.terms {
                    let known = shared
                        .terms
                        .iter()
                        .any(|t| t.term.to_lowercase() == term.term.to_lowercase());
                    if !known && !term.term.trim().is_empty() {
                        shared.terms.push(term);
                    }
                }
                (shared.notes.clone(), shared.terms.clone())
            };
            emit(&events, AssistEvent::NotesUpdated { notes });
            if !terms.is_empty() {
                emit(&events, AssistEvent::TermsUpdated { terms });
            }
        }));
    }

    fn ask(&self, kind: AskKind, text: String) {
        let Some(chat) = self.chat.clone() else {
            return report(&self.events, &AssistError::Chat("no chat provider".into()));
        };
        if !lock(&self.rate).try_acquire(Instant::now()) {
            return report(&self.events, &AssistError::RateLimited);
        }
        let messages = self.helper(kind, &text);
        let events = self.events.clone();
        self.runtime
            .spawn(stream_reply(chat, kind, messages, events));
    }

    fn helper(&self, kind: AskKind, text: &str) -> Vec<crate::llm::Message> {
        let shared = lock(&self.shared);
        helper_messages(kind, &self.context, &shared.transcript, &shared.notes, text)
    }

    /// Let pending notes land, write the end-of-class summary, then report `Off`.
    fn finish(mut self) {
        let has_text = !lock(&self.shared).transcript.segments.is_empty();
        let summary = match (&self.chat, has_text) {
            (Some(chat), true) => Some((chat.clone(), self.helper(AskKind::Summary, ""))),
            _ => None,
        };
        let notes_task = self.notes_task.take();
        let events = self.events.clone();
        self.runtime.spawn(async move {
            if let Some(task) = notes_task {
                let _ = task.await;
            }
            if let Some((chat, messages)) = summary {
                stream_reply(chat, AskKind::Summary, messages, events.clone()).await;
            }
            emit(&events, AssistEvent::StateChanged { state: State::Off });
        });
    }
}

async fn stream_reply(
    chat: Arc<dyn ChatProvider>,
    kind: AskKind,
    messages: Vec<crate::llm::Message>,
    events: UnboundedSender<AssistEvent>,
) {
    match chat.stream(messages).await {
        Ok(mut stream) => {
            while let Some(piece) = stream.next().await {
                match piece {
                    Ok(text) => emit(
                        &events,
                        AssistEvent::HelperReply {
                            kind,
                            text,
                            done: false,
                        },
                    ),
                    Err(error) => {
                        report(&events, &error);
                        break;
                    }
                }
            }
        }
        Err(error) => report(&events, &error),
    }
    emit(
        &events,
        AssistEvent::HelperReply {
            kind,
            text: String::new(),
            done: true,
        },
    );
}

fn join<'a>(texts: impl Iterator<Item = &'a str>) -> String {
    texts.collect::<Vec<_>>().join(" ")
}

/// whisper ignores audio under a second; pad short phrases with silence.
fn padded(audio: &[f32]) -> Vec<f32> {
    let mut out = audio.to_vec();
    let min = SAMPLE_RATE as usize * 11 / 10;
    if out.len() < min {
        out.resize(min, 0.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::fake::FakeSource;
    use crate::stt::Piece;
    use crate::vad::EnergyVad;

    struct Panics;

    impl Recognizer for Panics {
        fn transcribe(&mut self, _: &[f32], _: &str) -> Result<Vec<Piece>, AssistError> {
            panic!("recogniser bug")
        }
    }

    fn tone(seconds: f32) -> Vec<f32> {
        let n = (seconds * SAMPLE_RATE as f32) as usize;
        let mut out = vec![0.0; SAMPLE_RATE as usize];
        out.extend((0..n).map(|i| (i as f32 * 0.08).sin() * 0.3));
        out.extend(vec![0.0; SAMPLE_RATE as usize]);
        out
    }

    async fn states(mut events: UnboundedReceiver<AssistEvent>) -> Vec<State> {
        let mut states = Vec::new();
        while let Some(event) = events.recv().await {
            if let AssistEvent::StateChanged { state } = event {
                let end = matches!(state, State::Off | State::Error(_));
                states.push(state);
                if end {
                    break;
                }
            }
        }
        states
    }

    #[tokio::test]
    async fn a_failed_load_is_an_error_state() {
        let parts = Parts {
            source: Box::new(|| Err(AssistError::Audio("no device".into()))),
            recognizer: Box::new(|| Err(AssistError::Model("missing".into()))),
            vad: Box::new(|| Ok(Box::new(EnergyVad::default()) as Box<dyn Vad>)),
            chat: None,
        };
        let (_handle, events) =
            AssistEngine::start(AssistConfig::default(), ClassContext::default(), parts);
        assert_eq!(
            states(events).await,
            [State::Starting, State::Error("assist_error_model".into())]
        );
    }

    #[tokio::test]
    async fn a_panic_stays_inside_the_engine() {
        let parts = Parts {
            source: Box::new(|| Ok(Box::new(FakeSource::new(tone(1.5), 1600)) as Box<_>)),
            recognizer: Box::new(|| Ok(Box::new(Panics) as Box<dyn Recognizer>)),
            vad: Box::new(|| Ok(Box::new(EnergyVad::default()) as Box<dyn Vad>)),
            chat: None,
        };
        let (_handle, events) =
            AssistEngine::start(AssistConfig::default(), ClassContext::default(), parts);
        assert_eq!(
            states(events).await,
            [
                State::Starting,
                State::Listening,
                State::Error("assist_error_crash".into())
            ]
        );
    }

    #[test]
    fn short_audio_is_padded_to_a_second() {
        assert_eq!(padded(&[0.5; 100]).len(), 17_600);
        assert_eq!(padded(&[0.5; 20_000]).len(), 20_000);
    }
}
