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
| — | i5-14600KF, 32 GB, RTX 4080 SUPER | CPU | small | — | not run yet | | | | |

Not run yet: this machine has no speech model downloaded (the smallest in the catalogue,
`small`, is 488 MB), and fetching one waits for the owner's go-ahead. The command was checked
end to end up to loading the model, with a 17-second recording made by the Windows voice.
Fill the table from a real lecture recording of at least 10 minutes, once on CPU and once with
`cuda`.
