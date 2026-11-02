//! Only AutoPara's own sound: the class page in its tab plays from a WebView2 process that is a
//! child of the app, so a process loopback of the app's tree hears the class and nothing else
//! the computer plays (Windows 10 2004 and later).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::Duration;

use wasapi::{AudioClient, Direction, SampleType, StreamMode, WaveFormat};

use super::AudioSource;
use crate::{AssistError, SAMPLE_RATE};

enum Control {
    Pause,
    Resume,
}

pub struct AppAudio {
    samples: Receiver<Result<Vec<f32>, String>>,
    control: Sender<Control>,
}

impl AppAudio {
    /// Hear `process` and every process it started.
    pub fn open(process: u32) -> Self {
        let (sender, samples) = mpsc::channel();
        let (control, control_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("assist-app-audio".into())
            .spawn(move || {
                if let Err(error) = run(process, &sender, &control_rx) {
                    let _ = sender.send(Err(error.to_string()));
                }
            })
            .map_err(|e| tracing::error!(%e, "could not start the capture thread"))
            .ok();
        Self { samples, control }
    }
}

impl AudioSource for AppAudio {
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<f32>>, AssistError> {
        match self.samples.recv_timeout(timeout) {
            Ok(Ok(samples)) => Ok(Some(samples)),
            Ok(Err(reason)) => Err(AssistError::Audio(reason)),
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

/// Until the `AppAudio` is dropped. The audio engine converts to 16 kHz mono itself.
fn run(
    process: u32,
    samples: &Sender<Result<Vec<f32>, String>>,
    control: &Receiver<Control>,
) -> Result<(), wasapi::WasapiError> {
    let _ = wasapi::initialize_mta();
    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, 1, None);
    let mut client = AudioClient::new_application_loopback_client(process, true)?;
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };
    client.initialize_client(&format, &Direction::Capture, &mode)?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    let mut buffer = Vec::new();
    loop {
        match control.try_recv() {
            Ok(Control::Pause) => client.stop_stream()?,
            Ok(Control::Resume) => client.start_stream()?,
            Err(TryRecvError::Disconnected) => return Ok(()),
            Err(TryRecvError::Empty) => {}
        }
        // No event while the page is silent: nothing is played, so nothing comes.
        if event.wait_for_event(200).is_err() {
            continue;
        }
        while let Some(frames) = capture.get_next_packet_size()?.filter(|f| *f > 0) {
            buffer.resize(frames as usize * 4, 0);
            let (read, _) = capture.read_from_device(&mut buffer)?;
            let block = buffer[..read as usize * 4]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            if samples.send(Ok(block)).is_err() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays a system sound from a child process and hears it through this process's tree.
    #[test]
    #[ignore = "needs a sound device"]
    fn a_child_process_is_heard() {
        let mut source = AppAudio::open(std::process::id());
        std::thread::sleep(Duration::from_millis(300));
        let mut player = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(r"(New-Object Media.SoundPlayer 'C:\Windows\Media\tada.wav').PlaySync()")
            .spawn()
            .unwrap();
        let mut loudest = 0.0f32;
        while player.try_wait().unwrap().is_none() {
            if let Some(block) = source.read(Duration::from_millis(100)).unwrap() {
                loudest = block.iter().fold(loudest, |m, s| m.max(s.abs()));
            }
        }
        assert!(loudest > 0.01, "heard {loudest}");
    }
}
