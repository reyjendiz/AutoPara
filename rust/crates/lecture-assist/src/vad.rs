//! Voice activity detection and cutting the stream into utterances (plan step 7.4).

use crate::SAMPLE_RATE;

/// One VAD frame: 32 ms at 16 kHz, the frame Silero works in.
pub const FRAME: usize = 512;

/// Per-frame speech probabilities for a block of audio whose length is a multiple of [`FRAME`].
pub trait Vad: Send {
    fn probabilities(&mut self, audio: &[f32]) -> Vec<f32>;
}

/// The fallback detector: loudness against a slowly adapting noise floor. Good enough for a
/// lecture heard through the meeting app, where the other side is near silent between phrases.
pub struct EnergyVad {
    floor: f32,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self { floor: 1e-3 }
    }
}

impl Vad for EnergyVad {
    fn probabilities(&mut self, audio: &[f32]) -> Vec<f32> {
        audio
            .chunks(FRAME)
            .map(|frame| {
                let rms = rms(frame);
                let speech = rms > (self.floor * 3.0).max(0.006);
                // The floor follows quiet frames fast and loud ones slowly.
                let rate = if rms < self.floor { 0.2 } else { 0.005 };
                self.floor = (self.floor + (rms - self.floor) * rate).max(1e-4);
                if speech { 1.0 } else { 0.0 }
            })
            .collect()
    }
}

pub fn rms(audio: &[f32]) -> f32 {
    if audio.is_empty() {
        return 0.0;
    }
    (audio.iter().map(|s| s * s).sum::<f32>() / audio.len() as f32).sqrt()
}

/// Silero through whisper.cpp's own implementation, so no second inference runtime ships.
#[cfg(feature = "whisper")]
pub struct SileroVad {
    context: whisper_rs::WhisperVadContext,
    /// The last half second, so each block is judged with what came before it.
    history: Vec<f32>,
}

#[cfg(feature = "whisper")]
impl SileroVad {
    const HISTORY: usize = FRAME * 16;

    pub fn new(model: &std::path::Path) -> Result<Self, crate::AssistError> {
        let mut params = whisper_rs::WhisperVadContextParams::new();
        params.set_use_gpu(false);
        params.set_n_threads(1);
        let path = model.to_string_lossy();
        let context = whisper_rs::WhisperVadContext::new(&path, params)
            .map_err(|e| crate::AssistError::Model(e.to_string()))?;
        Ok(Self {
            context,
            history: Vec::new(),
        })
    }
}

#[cfg(feature = "whisper")]
impl Vad for SileroVad {
    fn probabilities(&mut self, audio: &[f32]) -> Vec<f32> {
        let frames = audio.len().div_ceil(FRAME);
        let mut input = std::mem::take(&mut self.history);
        input.extend_from_slice(audio);
        let probs = match self.context.detect_speech(&input) {
            Ok(()) => self.context.probabilities().to_vec(),
            Err(_) => Vec::new(),
        };
        self.history = input[input.len().saturating_sub(Self::HISTORY)..].to_vec();
        let mut tail: Vec<f32> = probs.iter().rev().take(frames).rev().copied().collect();
        tail.resize(frames, 0.0);
        tail
    }
}

/// A finished stretch of speech.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub audio: Vec<f32>,
    /// Offset of the first sample from the start of listening.
    pub start_sample: u64,
    /// Cut at the length limit rather than at a pause.
    pub forced: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SegmenterConfig {
    pub threshold: f32,
    /// Silence that ends an utterance.
    pub min_silence_frames: usize,
    /// Audio kept before the first speech frame.
    pub pre_roll_frames: usize,
    pub max_frames: usize,
    /// Carried into the next utterance after a forced cut.
    pub overlap_frames: usize,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        let frames = |ms: usize| ms * SAMPLE_RATE as usize / 1000 / FRAME;
        Self {
            threshold: 0.5,
            min_silence_frames: frames(700),
            pre_roll_frames: frames(200),
            max_frames: frames(28_000),
            overlap_frames: frames(500),
        }
    }
}

/// Groups frames into utterances: speech starts on the first frame over the threshold and ends
/// after enough silence, or is cut at the length limit with a small overlap.
pub struct Segmenter {
    config: SegmenterConfig,
    /// Samples seen so far, which dates every utterance.
    position: u64,
    pre_roll: Vec<f32>,
    open: Vec<f32>,
    open_start: u64,
    speaking: bool,
    silence: usize,
}

impl Segmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        Self {
            config,
            position: 0,
            pre_roll: Vec::new(),
            open: Vec::new(),
            open_start: 0,
            speaking: false,
            silence: 0,
        }
    }

    /// The speech heard so far in the utterance not yet finished.
    pub fn open(&self) -> Option<&[f32]> {
        self.speaking.then_some(self.open.as_slice())
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    /// Feed audio and its per-frame probabilities; returns the utterances it finished.
    pub fn push(&mut self, audio: &[f32], probs: &[f32]) -> Vec<Utterance> {
        let mut done = Vec::new();
        for (frame, prob) in audio.chunks(FRAME).zip(probs) {
            let speech = *prob >= self.config.threshold;
            if self.speaking {
                self.open.extend_from_slice(frame);
                self.silence = if speech { 0 } else { self.silence + 1 };
                if self.silence >= self.config.min_silence_frames {
                    // Leave out the trailing silence, but keep a little of it.
                    let keep = self.open.len()
                        - (self.silence - self.config.pre_roll_frames.min(self.silence)) * FRAME;
                    self.open.truncate(keep);
                    done.push(self.close(false));
                } else if self.open.len() >= self.config.max_frames * FRAME {
                    let overlap = self.config.overlap_frames * FRAME;
                    let tail = self.open[self.open.len() - overlap..].to_vec();
                    let end = self.position + frame.len() as u64;
                    done.push(self.close(true));
                    self.speaking = true;
                    self.open = tail;
                    self.open_start = end - overlap as u64;
                }
            } else if speech {
                self.speaking = true;
                self.silence = 0;
                self.open_start = self.position - self.pre_roll.len() as u64;
                self.open = std::mem::take(&mut self.pre_roll);
                self.open.extend_from_slice(frame);
            } else {
                self.pre_roll.extend_from_slice(frame);
                let excess = self
                    .pre_roll
                    .len()
                    .saturating_sub(self.config.pre_roll_frames * FRAME);
                self.pre_roll.drain(..excess);
            }
            self.position += frame.len() as u64;
        }
        done
    }

    /// Finish whatever is open (stop or pause).
    pub fn flush(&mut self) -> Option<Utterance> {
        self.speaking.then(|| self.close(false))
    }

    fn close(&mut self, forced: bool) -> Utterance {
        self.speaking = false;
        self.silence = 0;
        Utterance {
            audio: std::mem::take(&mut self.open),
            start_sample: self.open_start,
            forced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 220 Hz tone for speech, faint noise for the pauses.
    fn synth(pattern: &[(bool, usize)]) -> Vec<f32> {
        let mut out = Vec::new();
        let mut seed = 1u32;
        for &(loud, ms) in pattern {
            for _ in 0..ms * SAMPLE_RATE as usize / 1000 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 9) as f32 / (1u32 << 23) as f32 - 0.5;
                let t = out.len() as f32 / SAMPLE_RATE as f32;
                let tone = (t * 220.0 * std::f32::consts::TAU).sin() * 0.3;
                out.push(if loud { tone } else { 0.0 } + noise * 0.002);
            }
        }
        let whole = out.len() / FRAME * FRAME;
        out.truncate(whole);
        out
    }

    fn run(audio: &[f32], config: SegmenterConfig) -> (Vec<Utterance>, Segmenter) {
        let mut vad = EnergyVad::default();
        let mut segmenter = Segmenter::new(config);
        let mut out = Vec::new();
        // Arrive in uneven blocks, as from a sound card.
        for block in audio.chunks(FRAME * 7) {
            let probs = vad.probabilities(block);
            out.extend(segmenter.push(block, &probs));
        }
        (out, segmenter)
    }

    #[test]
    fn two_phrases_make_two_utterances() {
        let audio = synth(&[
            (false, 1000),
            (true, 1500),
            (false, 1200),
            (true, 2000),
            (false, 1000),
        ]);
        let (utterances, _) = run(&audio, SegmenterConfig::default());
        assert_eq!(utterances.len(), 2);
        let secs = |u: &Utterance| u.audio.len() as f32 / SAMPLE_RATE as f32;
        assert!(
            (1.5..2.2).contains(&secs(&utterances[0])),
            "{}",
            secs(&utterances[0])
        );
        assert!((2.0..2.7).contains(&secs(&utterances[1])));
        // The first starts near one second in, with its pre-roll.
        let start = utterances[0].start_sample as f32 / SAMPLE_RATE as f32;
        assert!((0.7..1.0).contains(&start), "{start}");
        assert!(utterances.iter().all(|u| !u.forced));
    }

    #[test]
    fn silence_gives_nothing() {
        let audio = synth(&[(false, 5000)]);
        let (utterances, segmenter) = run(&audio, SegmenterConfig::default());
        assert!(utterances.is_empty());
        assert!(segmenter.open().is_none());
    }

    #[test]
    fn long_speech_is_cut_with_overlap() {
        let config = SegmenterConfig {
            max_frames: 100,
            overlap_frames: 10,
            ..SegmenterConfig::default()
        };
        let audio = synth(&[(true, 8000), (false, 1000)]);
        let (utterances, _) = run(&audio, config);
        assert!(utterances.len() >= 2);
        assert!(utterances[0].forced);
        assert_eq!(utterances[0].audio.len(), 100 * FRAME);
        let first_end = utterances[0].start_sample + utterances[0].audio.len() as u64;
        assert_eq!(utterances[1].start_sample, first_end - 10 * FRAME as u64);
        assert!(!utterances.last().map(|u| u.forced).unwrap_or(true));
    }

    #[test]
    fn flush_closes_open_speech() {
        let audio = synth(&[(false, 500), (true, 1000)]);
        let (utterances, mut segmenter) = run(&audio, SegmenterConfig::default());
        assert!(utterances.is_empty());
        assert!(segmenter.open().is_some());
        assert!(segmenter.flush().is_some());
        assert!(segmenter.open().is_none());
    }
}
