//! Capture from a sound device through cpal: on Windows the loopback of an output device
//! (WASAPI), so the meeting app's sound is heard as the user hears it; elsewhere an input
//! device, such as BlackHole on macOS (plan steps 7.2, 7.3).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use ts_rs::TS;

use super::AudioSource;
use super::resample::To16k;
use crate::AssistError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "AudioDevice")]
pub struct Device {
    pub id: String,
    pub name: String,
    /// An output device heard through loopback.
    pub loopback: bool,
}

/// Output devices first on Windows (their loopback is what the user wants), then inputs.
pub fn devices() -> Vec<Device> {
    let host = cpal::default_host();
    let list = |devices: Result<_, _>, loopback| -> Vec<Device> {
        devices
            .map(|all: Box<dyn Iterator<Item = cpal::Device>>| {
                all.filter_map(|d| {
                    Some(Device {
                        id: d.id().ok()?.to_string(),
                        name: d.to_string(),
                        loopback,
                    })
                })
                .collect()
            })
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    if cfg!(windows) {
        out = list(
            host.output_devices()
                .map(|d| Box::new(d) as Box<dyn Iterator<Item = _>>),
            true,
        );
    }
    out.extend(list(
        host.input_devices()
            .map(|d| Box::new(d) as Box<dyn Iterator<Item = _>>),
        false,
    ));
    out
}

enum Event {
    Audio(Vec<f32>),
    Failed(String),
}

enum Control {
    Pause,
    Resume,
}

/// The cpal stream lives on its own thread (it is not `Send` everywhere), which also reopens
/// the device when it disappears or, when following the default, when the default changes.
pub struct Capture {
    events: Receiver<Event>,
    control: Sender<Control>,
}

impl Capture {
    /// `device`: an id from [`devices`], or `None` for the default (output on Windows).
    pub fn open(device: Option<String>) -> Self {
        let (event_tx, events) = mpsc::channel();
        let (control, control_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("assist-capture".into())
            .spawn(move || run(device, &event_tx, &control_rx))
            .map_err(|e| tracing::error!(%e, "could not start the capture thread"))
            .ok();
        Self { events, control }
    }
}

impl AudioSource for Capture {
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<f32>>, AssistError> {
        match self.events.recv_timeout(timeout) {
            Ok(Event::Audio(samples)) => Ok(Some(samples)),
            Ok(Event::Failed(reason)) => Err(AssistError::Audio(reason)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(AssistError::SourceEnded),
        }
    }

    fn set_paused(&mut self, paused: bool) {
        let _ = self.control.send(if paused {
            Control::Pause
        } else {
            Control::Resume
        });
    }
}

/// Until the `Capture` is dropped: keep a stream open, reopening it when it breaks.
fn run(device: Option<String>, events: &Sender<Event>, control: &Receiver<Control>) {
    let mut paused = false;
    loop {
        let lost = Arc::new(AtomicBool::new(false));
        let opened = open(device.as_deref(), events, &lost);
        let (stream, opened_id) = match opened {
            Ok(opened) => opened,
            Err(reason) => {
                let _ = events.send(Event::Failed(reason));
                // Wait for the device to come back, unless the caller has gone.
                match control.recv_timeout(Duration::from_secs(2)) {
                    Err(RecvTimeoutError::Disconnected) => return,
                    Ok(Control::Pause) => paused = true,
                    Ok(Control::Resume) => paused = false,
                    Err(RecvTimeoutError::Timeout) => {}
                }
                continue;
            }
        };
        let _ = if paused {
            stream.pause()
        } else {
            stream.play()
        };
        loop {
            match control.recv_timeout(Duration::from_secs(2)) {
                Err(RecvTimeoutError::Disconnected) => return,
                Ok(Control::Pause) => {
                    paused = true;
                    let _ = stream.pause();
                }
                Ok(Control::Resume) => {
                    paused = false;
                    let _ = stream.play();
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let default_moved = device.is_none() && default_id().as_ref() != Some(&opened_id);
            if lost.load(Ordering::Relaxed) || default_moved {
                tracing::info!("reopening the audio device");
                break;
            }
        }
    }
}

fn default_id() -> Option<String> {
    let host = cpal::default_host();
    let device = if cfg!(windows) {
        host.default_output_device()
    } else {
        host.default_input_device()
    };
    Some(device?.id().ok()?.to_string())
}

fn open(
    id: Option<&str>,
    events: &Sender<Event>,
    lost: &Arc<AtomicBool>,
) -> Result<(cpal::Stream, String), String> {
    let host = cpal::default_host();
    let device = match id {
        Some(id) => id
            .parse::<cpal::DeviceId>()
            .ok()
            .and_then(|id| host.device_by_id(&id)),
        None if cfg!(windows) => host.default_output_device(),
        None => host.default_input_device(),
    }
    .ok_or("the audio device was not found")?;
    let opened_id = device.id().map_err(|e| e.to_string())?.to_string();
    // An output device is captured through loopback, in its own mix format.
    let supported = if device.supports_input() {
        device.default_input_config()
    } else {
        device.default_output_config()
    }
    .map_err(|e| e.to_string())?;
    let mut resampler =
        To16k::new(supported.sample_rate(), supported.channels()).map_err(|e| e.to_string())?;
    let config = supported.config();
    let audio = events.clone();
    let failed = events.clone();
    let lost = lost.clone();
    let on_error = move |error: cpal::Error| {
        lost.store(true, Ordering::Relaxed);
        let _ = failed.send(Event::Failed(error.to_string()));
    };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            config,
            move |samples: &[f32], _: &cpal::InputCallbackInfo| {
                let out = resampler.process(samples);
                if !out.is_empty() {
                    let _ = audio.send(Event::Audio(out));
                }
            },
            on_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            config,
            move |samples: &[i16], _: &cpal::InputCallbackInfo| {
                let floats: Vec<f32> = samples.iter().map(|&s| f32::from(s) / 32768.0).collect();
                let out = resampler.process(&floats);
                if !out.is_empty() {
                    let _ = audio.send(Event::Audio(out));
                }
            },
            on_error,
            None,
        ),
        other => return Err(format!("unsupported sample format {other}")),
    }
    .map_err(|e| e.to_string())?;
    Ok((stream, opened_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No assertion on what is found: CI runners have no sound devices. It must not panic.
    #[test]
    fn listing_devices_never_panics() {
        for device in devices() {
            assert!(!device.id.is_empty());
        }
    }

    #[test]
    fn a_missing_device_reports_and_keeps_trying() {
        let mut capture = Capture::open(Some("nonsense:device".into()));
        let first = capture.read(Duration::from_secs(5));
        assert!(matches!(first, Err(AssistError::Audio(_))), "{first:?}");
    }
}
