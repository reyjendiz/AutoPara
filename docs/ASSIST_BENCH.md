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

Median latency was 16 s for both partials and finals (43 partials, 14 finals). The 5.6-minute
version of the same recording was stopped after 32 minutes without finishing.

**This misses the budget by a wide margin:** at these speeds the engine falls further behind
every second, because it asks for a partial per second of *audio* and capture keeps queueing
while a partial runs. Before release:

1. Find why one call takes ~16 s. The build is Release with AVX2/FMA/F16C, 8 threads, greedy
   decoding, so this is slower than whisper.cpp `small` usually is on this CPU. Suspects: the
   8 threads landing on E-cores (try 6, the P-core count), and temperature fallback re-decoding.
2. Make partials skip when the recogniser is behind (by wall time, not audio time), so the
   lag cannot grow.
3. Repeat with a real lecture recording, and with the `cuda` build on the RTX 4080 SUPER.

The recording is synthetic (the Windows English voice, monotone, regular pauses). Use it for
timing only, not for accuracy.
