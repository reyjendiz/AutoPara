//! Downmix and resample captured audio to 16 kHz mono `f32` (plan step 7.2).

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use crate::{AssistError, SAMPLE_RATE};

const CHUNK: usize = 1024;

pub struct To16k {
    channels: usize,
    /// `None` when the device already runs at 16 kHz.
    resampler: Option<Fft<f32>>,
    /// Mono samples waiting for a whole chunk.
    pending: Vec<f32>,
}

impl To16k {
    pub fn new(rate: u32, channels: u16) -> Result<Self, AssistError> {
        let resampler = if rate == SAMPLE_RATE {
            None
        } else {
            let fft = Fft::new(
                rate as usize,
                SAMPLE_RATE as usize,
                CHUNK,
                1,
                FixedSync::Input,
            )
            .map_err(|e| AssistError::Audio(e.to_string()))?;
            Some(fft)
        };
        Ok(Self {
            channels: usize::from(channels.max(1)),
            resampler,
            pending: Vec::new(),
        })
    }

    /// Interleaved device samples in, 16 kHz mono out; leftovers wait for the next call.
    pub fn process(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let mono = interleaved
            .chunks(self.channels)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32);
        let Some(resampler) = &mut self.resampler else {
            return mono.collect();
        };
        self.pending.extend(mono);
        let mut out = Vec::new();
        loop {
            let need = resampler.input_frames_next();
            if self.pending.len() < need {
                break;
            }
            if let Ok(input) = InterleavedSlice::new(&self.pending[..need], 1, need)
                && let Ok(buffer) = resampler.process(&input, None)
            {
                out.extend(buffer.take_data());
            }
            self.pending.drain(..need);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stereo_sine(rate: u32, hz: f32, seconds: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        (0..frames)
            .flat_map(|i| {
                let s = (i as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.5;
                [s, s]
            })
            .collect()
    }

    /// Sign changes per second, twice the frequency.
    fn crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count()
    }

    #[test]
    fn one_second_of_48k_stereo_is_one_second_of_16k() {
        let mut resampler = To16k::new(48_000, 2).unwrap();
        let mut out = Vec::new();
        // Device callbacks arrive in odd sizes.
        for block in stereo_sine(48_000, 440.0, 2.0).chunks(882) {
            out.extend(resampler.process(block));
        }
        let expected = 32_000;
        assert!(out.len().abs_diff(expected) <= CHUNK, "{}", out.len());
        let steady = &out[1000..17_000];
        assert!(
            crossings(steady).abs_diff(880) <= 4,
            "{}",
            crossings(steady)
        );
    }

    #[test]
    fn sixteen_k_is_only_downmixed() {
        let mut resampler = To16k::new(16_000, 2).unwrap();
        assert_eq!(resampler.process(&[0.2, 0.4, -1.0, 1.0]), [0.3, 0.0]);
    }

    #[test]
    fn odd_rates_work() {
        let mut resampler = To16k::new(44_100, 1).unwrap();
        let mono: Vec<f32> = stereo_sine(44_100, 300.0, 1.0)
            .into_iter()
            .step_by(2)
            .collect();
        let out = resampler.process(&mono);
        assert!(out.len().abs_diff(16_000) <= CHUNK, "{}", out.len());
    }
}
