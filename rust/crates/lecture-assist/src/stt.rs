//! Speech recognition: the `Recognizer` trait and the whisper backend (plan step 7.5).

use crate::AssistError;

/// Recognised text, timed from the start of the audio passed in.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

pub trait Recognizer: Send {
    /// `prompt` primes spelling (the user's glossary); see `text::initial_prompt`.
    fn transcribe(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<Piece>, AssistError>;

    /// The saved line of a finished utterance, read once; may take more care than a partial.
    fn transcribe_final(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<Piece>, AssistError> {
        self.transcribe(audio, prompt)
    }
}

/// The languages a class may be in; detection picks among these only.
#[cfg(feature = "whisper")]
/// Classes are in Ukrainian or Russian; English is still there to pick by hand.
const CANDIDATES: [&str; 2] = ["uk", "ru"];

#[cfg(feature = "whisper")]
pub struct Whisper {
    context: whisper_rs::WhisperContext,
    state: whisper_rs::WhisperState,
    language: crate::Language,
    /// Fixed once [`LanguageVote`] has heard enough: a class keeps its language.
    detected: Option<&'static str>,
    vote: LanguageVote,
    threads: i32,
    no_speech_threshold: f32,
    /// On the GPU the encoder reads whisper's whole 30 s window; see [`audio_context`].
    gpu: bool,
}

/// A Vulkan build loads `vulkan-1.dll` on first use (delay-loaded, see autopara-app's build.rs),
/// and any whisper.cpp call without it would crash, so callers check this first. GPU drivers of
/// every vendor install it; a computer with no graphics driver has none.
pub fn backend_available() -> bool {
    !cfg!(all(windows, feature = "vulkan"))
        || std::env::var_os("SystemRoot").is_some_and(|root| {
            std::path::Path::new(&root)
                .join("System32")
                .join("vulkan-1.dll")
                .is_file()
        })
}

#[cfg(feature = "whisper")]
impl Whisper {
    /// Loads the model; on the GPU when built with `cuda`, `vulkan` or `metal` and `gpu` is set.
    pub fn load(
        model: &std::path::Path,
        language: crate::Language,
        gpu: bool,
    ) -> Result<Self, AssistError> {
        let mut params = whisper_rs::WhisperContextParameters::default();
        params.use_gpu(gpu).flash_attn(gpu);
        let context = whisper_rs::WhisperContext::new_with_params(model, params)
            .map_err(|e| AssistError::Model(e.to_string()))?;
        let state = context
            .create_state()
            .map_err(|e| AssistError::Model(e.to_string()))?;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        Ok(Self {
            context,
            state,
            language,
            detected: None,
            vote: LanguageVote::default(),
            threads,
            no_speech_threshold: 0.6,
            gpu,
        })
    }

    /// How likely each of [`CANDIDATES`] is. It runs on a state of its own: detecting on the
    /// recogniser's state left every later window garbled ("a a a a" for clear Ukrainian, measured
    /// on a LibriVox recording).
    fn detect(&self, audio: &[f32]) -> Option<[f32; 2]> {
        let threads = self.threads as usize;
        let mut probe = self.context.create_state().ok()?;
        probe.pcm_to_mel(audio, threads).ok()?;
        let (_, probs) = probe.lang_detect(0, threads).ok()?;
        Some(CANDIDATES.map(|l| {
            whisper_rs::get_lang_id(l)
                .and_then(|id| probs.get(id as usize).copied())
                .unwrap_or(0.0)
        }))
    }
}

#[cfg(feature = "whisper")]
impl Recognizer for Whisper {
    fn transcribe(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<Piece>, AssistError> {
        self.recognise(
            audio,
            prompt,
            whisper_rs::SamplingStrategy::Greedy { best_of: 1 },
        )
    }

    /// Beam search on the GPU: 1.5-3.5 points fewer errors on a real lecture than greedy, for
    /// a final p95 of 1.0 s instead of 0.36 s (docs/ASSIST_BENCH.md). Partials stay greedy, and the
    /// CPU build too, where the decoder is the slow part.
    fn transcribe_final(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<Piece>, AssistError> {
        let strategy = if self.gpu {
            whisper_rs::SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: -1.0,
            }
        } else {
            whisper_rs::SamplingStrategy::Greedy { best_of: 1 }
        };
        self.recognise(audio, prompt, strategy)
    }
}

#[cfg(feature = "whisper")]
impl Whisper {
    fn recognise(
        &mut self,
        audio: &[f32],
        prompt: &str,
        strategy: whisper_rs::SamplingStrategy,
    ) -> Result<Vec<Piece>, AssistError> {
        let language = match self.language.code() {
            Some(code) => code,
            None => match self.detected {
                Some(code) => code,
                None => {
                    if let Some(probabilities) = self.detect(audio) {
                        self.vote.add(audio, probabilities);
                    }
                    let leader = self.vote.leader();
                    if self.vote.settled() {
                        self.detected = Some(leader);
                    }
                    leader
                }
            },
        };
        let mut params = whisper_rs::FullParams::new(strategy);
        params.set_language(Some(language));
        params.set_initial_prompt(prompt);
        params.set_n_threads(self.threads);
        if !self.gpu {
            params.set_audio_ctx(audio_context(audio.len()));
        }
        params.set_no_speech_thold(self.no_speech_threshold);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        // Each call is independent: carrying context between windows feeds repetition loops.
        params.set_no_context(true);
        // One decoding pass: the temperature fallback re-decodes doubtful noisy audio up to five
        // times, which is where a slow class came from; repetitions are cleaned up afterwards.
        params.set_temperature_inc(0.0);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        self.state
            .full(params, audio)
            .map_err(|e| AssistError::Recognition(e.to_string()))?;
        let mut pieces = Vec::new();
        for segment in self.state.as_iter() {
            if segment.no_speech_probability() > self.no_speech_threshold {
                continue;
            }
            let text = segment
                .to_str_lossy()
                .map_or_else(|_| "".into(), |t| t.trim().to_string());
            if text.is_empty() {
                continue;
            }
            // whisper counts in hundredths of a second.
            pieces.push(Piece {
                start_ms: segment.start_timestamp().max(0) as u64 * 10,
                end_ms: segment.end_timestamp().max(0) as u64 * 10,
                text,
            });
        }
        Ok(pieces)
    }
}

/// Speech heard before the class language is fixed. One phrase was too little: a quoted line of
/// verse, or the same lecture through a laptop microphone, Opus or a noisy room, read as Russian,
/// and then the whole Ukrainian class came out in Russian (WER 31% -> 82-98%, docs/ASSIST_BENCH.md).
#[cfg(feature = "whisper")]
const VOTE_SECONDS: f32 = 60.0;

/// How much more Russian must score. Whisper hears degraded Ukrainian as Russian (up to 1.2 times
/// the Ukrainian score over a minute), never the other way round: degraded Russian scored 2.4 to
/// 38 times Ukrainian.
#[cfg(feature = "whisper")]
const RUSSIAN_MARGIN: f32 = 1.5;

/// Whisper's language guesses, weighted by the length of audio they were made on.
#[cfg(feature = "whisper")]
#[derive(Debug, Default)]
struct LanguageVote {
    heard: f32,
    scores: [f32; 2],
    /// The last call's start and vote: a partial re-reads its utterance every second, so a call
    /// that starts the same way replaces that vote instead of counting the audio again.
    last: Option<(Vec<f32>, f32, [f32; 2])>,
}

#[cfg(feature = "whisper")]
impl LanguageVote {
    /// A quarter of a second tells one utterance from the next.
    const HEAD: usize = crate::SAMPLE_RATE as usize / 4;

    fn add(&mut self, audio: &[f32], probabilities: [f32; 2]) {
        let head = &audio[..audio.len().min(Self::HEAD)];
        if let Some((last_head, seconds, last)) = &self.last
            && last_head.as_slice() == head
        {
            self.heard -= seconds;
            self.scores = [0, 1].map(|i| self.scores[i] - seconds * last[i]);
        }
        let seconds = audio.len() as f32 / crate::SAMPLE_RATE as f32;
        self.heard += seconds;
        self.scores = [0, 1].map(|i| self.scores[i] + seconds * probabilities[i]);
        self.last = Some((head.to_vec(), seconds, probabilities));
    }

    /// Ukrainian until Russian clearly scores higher.
    fn leader(&self) -> &'static str {
        CANDIDATES[usize::from(self.scores[1] > RUSSIAN_MARGIN * self.scores[0])]
    }

    fn settled(&self) -> bool {
        self.heard >= VOTE_SECONDS
    }
}

/// Encoder frames for the audio (50 a second, plus a margin) instead of whisper's fixed 30 s:
/// encoding the silent padding made a 3 s partial cost as much as a full window. CPU only: with a
/// cut window whisper repeated or dropped phrases ("Сколько лет... Сколько лет...", "Н." for a whole
/// question, 2026-11-03), and on the GPU the whole window costs ~85 ms a phrase.
#[cfg(feature = "whisper")]
fn audio_context(samples: usize) -> i32 {
    let frames = samples * 50 / crate::SAMPLE_RATE as usize + 64;
    frames.min(1500) as i32
}

/// Plays back fixed text, for tests: each call returns the next entry.
pub struct Scripted(pub std::collections::VecDeque<String>);

impl Recognizer for Scripted {
    fn transcribe(&mut self, audio: &[f32], _prompt: &str) -> Result<Vec<Piece>, AssistError> {
        let end_ms = audio.len() as u64 * 1000 / u64::from(crate::SAMPLE_RATE);
        Ok(self
            .0
            .pop_front()
            .map(|text| {
                vec![Piece {
                    start_ms: 0,
                    end_ms,
                    text,
                }]
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_times_its_pieces() {
        let mut recognizer = Scripted(["Привіт".to_string()].into());
        let pieces = recognizer.transcribe(&[0.0; 8000], "").unwrap();
        assert_eq!(pieces[0].end_ms, 500);
        assert!(recognizer.transcribe(&[0.0; 10], "").unwrap().is_empty());
    }

    #[cfg(feature = "whisper")]
    #[test]
    fn one_russian_sounding_phrase_does_not_settle_a_ukrainian_class() {
        let mut vote = LanguageVote::default();
        let phrase = |seed: f32, seconds: usize| vec![seed; seconds * 16_000];
        vote.add(&phrase(0.1, 5), [0.14, 0.76]);
        assert_eq!(vote.leader(), "ru");
        assert!(!vote.settled());
        for k in 0..7 {
            assert!(!vote.settled());
            vote.add(&phrase(0.2 + k as f32 / 10.0, 8), [0.5, 0.6]);
        }
        assert!(vote.settled());
        assert_eq!(vote.leader(), "uk");
    }

    #[cfg(feature = "whisper")]
    #[test]
    fn a_russian_class_is_still_russian() {
        let mut vote = LanguageVote::default();
        for k in 0..6 {
            vote.add(&vec![k as f32; 10 * 16_000], [0.25, 0.65]);
        }
        assert!(vote.settled());
        assert_eq!(vote.leader(), "ru");
    }

    #[cfg(feature = "whisper")]
    #[test]
    fn partials_of_one_utterance_vote_once() {
        let mut vote = LanguageVote::default();
        let utterance: Vec<f32> = (0..10 * 16_000).map(|i| (i % 7) as f32).collect();
        for second in 1..=10 {
            vote.add(&utterance[..second * 16_000], [0.2, 0.8]);
        }
        assert_eq!(vote.heard, 10.0);
        assert!((vote.scores[1] - 8.0).abs() < 1e-3);
        vote.add(&[0.5; 16_000], [1.0, 0.0]);
        assert_eq!(vote.heard, 11.0);
    }

    #[cfg(feature = "whisper")]
    #[test]
    fn short_audio_encodes_a_short_window() {
        assert_eq!(audio_context(3 * 16_000), 214);
        assert_eq!(audio_context(40 * 16_000), 1500);
    }
}
