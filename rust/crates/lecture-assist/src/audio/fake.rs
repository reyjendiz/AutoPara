//! A scripted audio source for tests and the benchmark (plan step 7.13).

use std::path::Path;
use std::time::Duration;

use super::AudioSource;
use crate::{AssistError, SAMPLE_RATE};

/// Hands out prepared samples as fast as they are read, then ends.
pub struct FakeSource {
    samples: Vec<f32>,
    position: usize,
    chunk: usize,
}

impl FakeSource {
    pub fn new(samples: Vec<f32>, chunk: usize) -> Self {
        Self {
            samples,
            position: 0,
            chunk: chunk.max(1),
        }
    }

    /// A 16 kHz mono wav file, in blocks of 100 ms.
    pub fn from_wav(path: &Path) -> Result<Self, AssistError> {
        let reader = hound::WavReader::open(path).map_err(|e| AssistError::Audio(e.to_string()))?;
        let spec = reader.spec();
        if spec.sample_rate != SAMPLE_RATE || spec.channels != 1 {
            return Err(AssistError::Audio(format!(
                "{}: need 16 kHz mono, got {} Hz × {}",
                path.display(),
                spec.sample_rate,
                spec.channels
            )));
        }
        let samples: Result<Vec<f32>, _> = match spec.sample_format {
            hound::SampleFormat::Float => reader.into_samples::<f32>().collect(),
            hound::SampleFormat::Int => {
                let scale = (1_i64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .map(|s| s.map(|s| s as f32 / scale))
                    .collect()
            }
        };
        let samples = samples.map_err(|e| AssistError::Audio(e.to_string()))?;
        Ok(Self::new(samples, SAMPLE_RATE as usize / 10))
    }
}

impl AudioSource for FakeSource {
    fn read(&mut self, _timeout: Duration) -> Result<Option<Vec<f32>>, AssistError> {
        if self.position >= self.samples.len() {
            return Err(AssistError::SourceEnded);
        }
        let end = (self.position + self.chunk).min(self.samples.len());
        let block = self.samples[self.position..end].to_vec();
        self.position = end;
        Ok(Some(block))
    }

    fn set_paused(&mut self, _paused: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hands_out_blocks_then_ends() {
        let mut source = FakeSource::new(vec![0.1; 5], 2);
        let mut lengths = Vec::new();
        while let Ok(Some(block)) = source.read(Duration::ZERO) {
            lengths.push(block.len());
        }
        assert_eq!(lengths, [2, 2, 1]);
        assert!(matches!(
            source.read(Duration::ZERO),
            Err(AssistError::SourceEnded)
        ));
    }

    #[test]
    fn reads_16k_mono_wav() {
        let dir = std::env::temp_dir().join(format!("assist-fake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tone.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..3200 {
            writer
                .write_sample(if i % 2 == 0 { 16_384_i16 } else { -16_384 })
                .unwrap();
        }
        writer.finalize().unwrap();
        let mut source = FakeSource::from_wav(&path).unwrap();
        let first = source.read(Duration::ZERO).unwrap().unwrap();
        assert_eq!(first.len(), 1600);
        assert!((first[0] - 0.5).abs() < 1e-4);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
