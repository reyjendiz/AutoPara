//! System audio on macOS 13+ through ScreenCaptureKit, audio only (plan step 7.3). Needs the
//! Screen Recording permission, so macOS shows its own recording indicator while it runs.
//!
//! Untested on hardware: written against screencapturekit 11 and only compiled by the macOS CI
//! job. Without the permission, `open` fails and the app falls back to an input device
//! (BlackHole).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::time::Duration;

use screencapturekit::prelude::*;

use super::AudioSource;
use crate::{AssistError, SAMPLE_RATE};

/// About ten seconds of the system's 10 ms blocks; later blocks are dropped, not queued forever.
const QUEUE: usize = 1024;

struct Sink(SyncSender<Vec<f32>>);

impl SCStreamOutputTrait for Sink {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, kind: SCStreamOutputType) {
        if !matches!(kind, SCStreamOutputType::Audio) {
            return;
        }
        let Ok(list) = sample.audio_buffer_list() else {
            return;
        };
        for buffer in &list {
            // One channel of 32-bit floats, as configured below.
            let samples: Vec<f32> = buffer
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| f32::from_ne_bytes(*bytes))
                .collect();
            if !samples.is_empty() {
                let _ = self.0.try_send(samples);
            }
        }
    }
}

pub struct ScreenAudio {
    stream: SCStream,
    receiver: Receiver<Vec<f32>>,
}

fn audio_error(error: SCError) -> AssistError {
    AssistError::Audio(format!("ScreenCaptureKit: {error}"))
}

impl ScreenAudio {
    /// Start capturing everything the Mac plays, already 16 kHz mono.
    pub fn open() -> Result<Self, AssistError> {
        // Fails when the Screen Recording permission has not been granted.
        let content = SCShareableContent::get().map_err(audio_error)?;
        let display = content
            .displays()
            .into_iter()
            .next()
            .ok_or_else(|| AssistError::Audio("ScreenCaptureKit: no display".into()))?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(audio_error)?;
        // The stream must have a picture; keep it tiny and rare.
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_minimum_frame_interval(&CMTime::new(1, 1))
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(SAMPLE_RATE as i32)
            .with_channel_count(1);
        let (sender, receiver) = mpsc::sync_channel(QUEUE);
        let mut stream = SCStream::new(&filter, &config).map_err(audio_error)?;
        stream
            .add_output_handler(Sink(sender), SCStreamOutputType::Audio)
            .map_err(audio_error)?;
        stream.start_capture().map_err(audio_error)?;
        tracing::info!("capturing system audio through ScreenCaptureKit");
        Ok(Self { stream, receiver })
    }
}

impl AudioSource for ScreenAudio {
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<f32>>, AssistError> {
        match self.receiver.recv_timeout(timeout) {
            Ok(block) => Ok(Some(block)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(AssistError::SourceEnded),
        }
    }

    fn set_paused(&mut self, paused: bool) {
        let result = if paused {
            self.stream.stop_capture()
        } else {
            self.stream.start_capture()
        };
        if let Err(error) = result {
            tracing::warn!(%error, paused, "could not change ScreenCaptureKit capture");
        }
    }
}

impl Drop for ScreenAudio {
    fn drop(&mut self) {
        let _ = self.stream.stop_capture();
    }
}
