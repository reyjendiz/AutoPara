# Lecture Assist: speech recognition benchmark

`cargo xtask bench-stt <recording.wav> [model.bin]` runs a 16 kHz mono recording through the
recogniser the way the engine does, only as fast as the machine allows:

- the energy voice detector and the engine's segmenter cut the audio into utterances
  (700 ms of silence ends one, 28 s is the longest);
- every second, the open utterance is transcribed again (the partial the overlay shows);
- each finished utterance is transcribed once more (the final line that is saved).

It prints the model load time, the real-time factor (busy time ÷ audio length, finals alone and
with partials), the median, p95 and maximum partial and final latency, and the process's peak
memory. It never prints the recognised text.

Without a model path it uses the default model in the data folder's `models/` (honours
`AUTOPARA_DATA_DIR`). xtask runs the `bench_stt` example of `lecture-assist` in release mode;
for a GPU build, run the example directly with `--features cuda` (or `metal`):

```
cargo run --release -p lecture-assist --features cuda --example bench_stt -- lecture.wav model.bin
```

A real-time factor under 1.0 with partials means the engine keeps up; the partial latency is how
late the grey line in the overlay runs behind the teacher.

## Results

| Date | Machine | Build | Model | Audio | RTF finals | RTF with partials | Partial p95 | Final p95 | Peak memory |
|------|---------|-------|-------|-------|-----------|-------------------|-------------|-----------|-------------|
| 2026-10-28 | i5-14600KF, 32 GB | CPU, release, AVX2 | small | 60 s, Windows voice (en) | 4.17 | 16.9 | 30.5 s | 28.9 s | 793 MB |
| 2026-10-28 | i5-14600KF, 32 GB | CPU, release, AVX2, `/O2`, audio-sized window | small | 60 s, Windows voice (en) | 0.21 | 0.66 | 1.85 s | 2.13 s | 792 MB |
| 2026-10-28 | i5-14600KF, 32 GB | CPU, release, AVX2, `/O2`, audio-sized window | small | 5.6 min, Windows voice (en) | 0.19 | 0.70 | 1.99 s | 2.20 s | 828 MB |

The first row (median 16 s per call) was an unoptimised whisper.cpp: with the Visual Studio
generator, cmake-rs replaces CMake's MSVC release flags with its own and drops `/O2`, so ggml
was compiled without optimisation. `rust/.cargo/config.toml` now sets `CMAKE_C_FLAGS_RELEASE`
and `CMAKE_CXX_FLAGS_RELEASE` (whisper-rs-sys forwards them to CMake). That alone gave RTF 0.55
for finals and 2.3 with partials, median 2 s per call.

The rest was whisper's fixed 30 s window: a 3 s partial still encoded 27 s of silence. The
recogniser now sizes the encoder window to the audio (`audio_ctx`, 50 frames a second plus a
margin): a 3 s call went from 1.7 s to 0.3 s with the same or more text; a 28 s call is unchanged
(~4 s). Threads (6 P-cores vs 8) and the temperature fallback made no measurable difference.
Language detection runs once per class (cached), not per call.

The bench times every partial, so "with partials" is a worst case: the engine now skips a partial
while more than a second of captured audio is waiting (wall time against audio heard), so a slow
stretch cannot make the lag grow; finals always run.

To do:

1. Repeat with a real lecture recording (accuracy and timing), Ukrainian and Russian included.
2. Repeat with the `cuda` build on the RTX 4080 SUPER.

The recording is synthetic (the Windows English voice, monotone, regular pauses). Use it for
timing only, not for accuracy.
