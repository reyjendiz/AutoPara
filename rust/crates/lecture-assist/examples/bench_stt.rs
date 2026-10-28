//! `cargo xtask bench-stt <wav> [model.bin]` (docs/ASSIST_BENCH.md): a recording through the
//! recogniser the way the engine does (energy detector, the engine's segmenter, a partial on the
//! open utterance every second, a final per utterance), but as fast as it goes. Prints timings
//! only, never the text.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use lecture_assist::audio::AudioSource;
use lecture_assist::audio::fake::FakeSource;
use lecture_assist::models::{self, ModelStore};
use lecture_assist::stt::{Recognizer, Whisper};
use lecture_assist::vad::{EnergyVad, Segmenter, SegmenterConfig, Vad};

const USAGE: &str = "usage: bench_stt <recording.wav> <model.bin | data folder>";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match bench(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bench-stt: {message}");
            ExitCode::FAILURE
        }
    }
}

fn bench(args: &[String]) -> Result<(), String> {
    const RATE: usize = 16_000;
    let [wav, model] = args else {
        return Err(USAGE.into());
    };
    // A data folder means its default model.
    let mut model = PathBuf::from(model);
    if model.is_dir() {
        let default = models::model(models::default_speech_model()).ok_or("no default model")?;
        model = ModelStore::new(model.join("models")).path(default);
    }
    let mut source = FakeSource::from_wav(Path::new(wav)).map_err(|e| e.to_string())?;
    let mut audio = Vec::new();
    while let Ok(Some(block)) = source.read(Duration::ZERO) {
        audio.extend(block);
    }
    let seconds = audio.len() as f64 / RATE as f64;
    let gpu = cfg!(any(feature = "cuda", feature = "metal"));
    let started = Instant::now();
    let mut recognizer = Whisper::load(&model, lecture_assist::Language::Auto, gpu)
        .map_err(|e| format!("{}: {e}", model.display()))?;
    println!(
        "model {} loaded in {:.1} s",
        model.display(),
        started.elapsed().as_secs_f64()
    );

    let mut time = |samples: &[f32]| -> Result<f64, String> {
        let started = Instant::now();
        recognizer
            .transcribe(samples, "")
            .map_err(|e| e.to_string())?;
        Ok(started.elapsed().as_secs_f64())
    };
    let (mut vad, mut segmenter) = (
        EnergyVad::default(),
        Segmenter::new(SegmenterConfig::default()),
    );
    let (mut partials, mut finals) = (Vec::new(), Vec::new());
    for second in audio.chunks(RATE) {
        let probs = vad.probabilities(second);
        let done = segmenter.push(second, &probs);
        for utterance in &done {
            finals.push(time(&utterance.audio)?);
        }
        if let Some(open) = segmenter.open().filter(|open| open.len() >= RATE) {
            partials.push(time(open)?);
        }
    }
    if let Some(last) = segmenter.flush() {
        finals.push(time(&last.audio)?);
    }

    let stats = |name: &str, list: &mut Vec<f64>| {
        list.sort_by(f64::total_cmp);
        let at = |q: f64| {
            list.get(((list.len() as f64 - 1.0) * q).round() as usize)
                .copied()
                .unwrap_or(0.0)
        };
        println!(
            "{name}: {} runs, median {:.2} s, p95 {:.2} s, max {:.2} s",
            list.len(),
            at(0.5),
            at(0.95),
            at(1.0)
        );
    };
    let busy: f64 = partials.iter().chain(&finals).sum();
    let final_busy: f64 = finals.iter().sum();
    println!("audio {seconds:.1} s, gpu {gpu}");
    println!(
        "real-time factor: finals {:.3}, with partials {:.3}",
        final_busy / seconds,
        busy / seconds
    );
    stats("partial latency", &mut partials);
    stats("final latency", &mut finals);
    match peak_memory_mb() {
        Some(mb) => println!("peak memory {mb} MB"),
        None => println!("peak memory unknown"),
    }
    Ok(())
}

/// This process's peak working set (Windows) or resident size (elsewhere), asked of the OS tools.
fn peak_memory_mb() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = if cfg!(windows) {
        let script = format!("(Get-Process -Id {pid}).PeakWorkingSet64");
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
    } else {
        Command::new("ps").args(["-o", "rss=", "-p", &pid]).output()
    }
    .ok()?;
    let value: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    Some(if cfg!(windows) {
        value / 1_048_576
    } else {
        value / 1024
    })
}
