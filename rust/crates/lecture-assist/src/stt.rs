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
    /// `prompt` primes spelling (class, teacher, glossary); see `text::initial_prompt`.
    fn transcribe(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<Piece>, AssistError>;
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
    /// Fixed after the first long enough utterance: a class keeps its language.
    detected: Option<&'static str>,
    threads: i32,
    no_speech_threshold: f32,
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
            threads,
            no_speech_threshold: 0.6,
        })
    }

    /// The likeliest of [`CANDIDATES`]; needs a few seconds of speech to be worth trusting. It runs
    /// on a state of its own: detecting on the recogniser's state left every later window garbled
    /// ("a a a a" for clear Ukrainian, measured on a LibriVox recording).
    fn detect(&self, audio: &[f32]) -> Option<&'static str> {
        let threads = self.threads as usize;
        let mut probe = self.context.create_state().ok()?;
        probe.pcm_to_mel(audio, threads).ok()?;
        let (_, probs) = probe.lang_detect(0, threads).ok()?;
        CANDIDATES.into_iter().max_by(|a, b| {
            let p = |l: &str| {
                whisper_rs::get_lang_id(l)
                    .and_then(|id| probs.get(id as usize).copied())
                    .unwrap_or(0.0)
            };
            p(a).total_cmp(&p(b))
        })
    }
}

#[cfg(feature = "whisper")]
impl Recognizer for Whisper {
    fn transcribe(&mut self, audio: &[f32], prompt: &str) -> Result<Vec<Piece>, AssistError> {
        let language = match self.language.code() {
            Some(code) => Some(code),
            None if self.detected.is_some() => self.detected,
            None if audio.len() >= 3 * crate::SAMPLE_RATE as usize => {
                self.detected = self.detect(audio);
                self.detected
            }
            None => None,
        };
        let mut params =
            whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        // Until a long enough utterance settles the class language, assume the usual one:
        // whisper's own guess on a short phrase was often wrong (Russian for English speech).
        params.set_language(Some(language.unwrap_or("uk")));
        params.set_initial_prompt(prompt);
        params.set_n_threads(self.threads);
        params.set_audio_ctx(audio_context(audio.len()));
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

/// Encoder frames for the audio (50 a second, plus a margin) instead of whisper's fixed 30 s:
/// encoding the silent padding made a 3 s partial cost as much as a full window.
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
    fn short_audio_encodes_a_short_window() {
        assert_eq!(audio_context(3 * 16_000), 214);
        assert_eq!(audio_context(40 * 16_000), 1500);
    }
}
