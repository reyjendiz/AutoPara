# Lecture Assist: speech recognition benchmark

`cargo xtask bench-stt <recording.wav> [model.bin]` runs a 16 kHz mono recording through the
recogniser the way the engine does, only as fast as the machine allows:

- the energy voice detector and the engine's segmenter cut the audio into utterances
  (700 ms of silence ends one, 20 s is the longest);
- every second, the open utterance is transcribed again (the partial the overlay shows);
- each finished utterance is transcribed once more (the final line that is saved).

It prints the model load time, the real-time factor (busy time ÷ audio length, finals alone and
with partials), the median, p95 and maximum partial and final latency, and the process's peak
memory. It never prints the recognised text.

Without a model path it uses the default model in the data folder's `models/` (honours
`AUTOPARA_DATA_DIR`). xtask runs the `bench_stt` example of `lecture-assist` in release mode;
for a GPU build, run the example directly with `--features vulkan` (or `cuda`, `metal`); a Vulkan
build needs `VULKAN_SDK` and a short `CARGO_TARGET_DIR` (see CONTRIBUTING.md):

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
| 2026-10-28 | i5-14600KF, RTX 4080 SUPER | CUDA 13.4, release, audio-sized window | small | 5.6 min, Windows voice (en) | 0.10 | 0.54 | 2.55 s | 2.55 s | 523 MB |
| 2026-11-03 | i5-14600KF, 32 GB | CPU, release | large-v3-turbo-q5_0 | 60 s, Windows voice (en) | 0.49 | 1.63 | 3.05 s | 3.26 s | 787 MB |
| 2026-11-03 | i5-14600KF, RTX 4080 SUPER | CUDA 13.4, release | large-v3-turbo-q5_0 | 60 s, Windows voice (en) | 0.20 | 0.85 | 2.74 s | 2.00 s | 488 MB |
| 2026-11-03 | i5-14600KF, RTX 4080 SUPER | Vulkan (SDK 1.4.363), release | large-v3-turbo-q5_0 | 60 s, Windows voice (en) | 0.13 | 0.93 | 3.34 s | 1.41 s | 520 MB |
| 2026-11-03 | i5-14600KF, RTX 4080 SUPER | Vulkan, 28 s cuts | large-v3-turbo-q5_0 | 90 s without pauses (en) | 0.031 | 0.65 | 1.91 s | 1.61 s | 290 MB |
| 2026-11-03 | i5-14600KF, RTX 4080 SUPER | Vulkan, 10 s cuts, no temperature fallback | large-v3-turbo-q5_0 | 90 s without pauses (en) | 0.015 | 0.13 | 0.30 s | 0.32 s | 259 MB |

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
2. On CUDA the median call is 0.10 s but p95 is 2.5 s for both partials and finals; find what
   makes the slow tail (likely decoding, not the encoder), and time `large-v3-turbo`, the GPU default.

The recording is synthetic (the Windows English voice, monotone, regular pauses). Use it for
timing only, not for accuracy.

## Accuracy under bad microphones (2026-11-04)

Owner: "it still recognises the teacher's words very badly in real classes". Word error rate (WER,
word-level Levenshtein after lower-casing and removing punctuation and apostrophes) through the
app's chain: 16 kHz mono, Silero, the engine's segmenter, `stt::Whisper` with `Language::Auto` on
Vulkan, `text::clean`; only the saved (final) lines are scored. Recordings and scripts stay
outside the repository.

- **Lecture:** 7 min (10:00-17:00) of M. Ilnytskyi's talk on Ye. Malaniuk, Lviv 2017
  (archive.org `Ilnyckyi-Malaniuk`, CC BY-NC-SA 4.0): one speaker in a bookstore, room microphone,
  quoted verse. No transcript exists, so the reference is whisper large-v3 (f16, beam 5, whole
  recording with context) on the clean audio, with names corrected by hand (Теліга, Білецький,
  Сосюра, празька школа...). Models of the large-v3 family are favoured by it; compare them on
  FLEURS.
- **FLEURS uk:** 40 sentences of the dev set (CC BY 4.0), one recording each, 0.8 s apart
  (8.3 min): read speech, many speakers, human transcripts.
- **Russian:** 4 min of the LibriVox reading the earlier probes used, reference by large-v3 (ru).

Conditions (ffmpeg and numpy): `laptop` 300-3400 Hz, speech at -30 dBFS, hiss 30 dB below;
`reverb` RT60 0.7 s, `reverb-light` 0.4 s; `babble` six other FLEURS voices at 20/10/5 dB SNR;
`fan10` brown noise at 10 dB; `clip` the top 1% of samples clipped; `opus16/24` libopus voip at
16/24 kbit/s; `loss5` 5% of 20 ms frames lost in bursts, no concealment; `meet` laptop + light
room + babble 20 dB + Opus 16 + 3% loss; `combo` laptop + reverb + babble 10 dB + Opus 16 + 3%;
`worst` adds clipping and 5% loss at 5 dB.

Before = the settings until 2026-11-03 (10 s cuts, greedy, language fixed on the first phrase of
3 s); after = this commit (language voted over a minute with a margin for Russian, 20 s cuts,
beam 5 on finals, lone "Дякую." dropped). large-v3-turbo-q5_0, RTX 4080 SUPER.

| WER, % | clean | laptop | reverb | reverb-light | babble 20 | babble 10 | babble 5 | fan 10 | clip | opus 16 | opus 24 | opus 24 + loss 5 | meet | combo | worst |
|--------|------:|-------:|-------:|-------------:|----------:|----------:|---------:|-------:|-----:|--------:|--------:|-----------------:|-----:|------:|------:|
| Lecture, before | 30.3 | 86.2 | 97.6 | 85.7 | 30.7 | 92.8 | 97.1 | 87.4 | 83.8 | 85.9 | 82.4 | 86.1 | 96.0 | 98.1 | 98.7 |
| Lecture, after  | 21.4 | 25.1 | 49.6 | 31.8 | 24.5 | 54.0 | 90.5 | 33.1 | 23.1 | 26.0 | 23.5 | 24.8 | 96.1 | 99.2 | 99.1 |
| FLEURS, before  | 14.6 | 31.9 | 38.5 | 28.5 | 45.4 | 97.2 | 94.7 | 46.4 | 14.4 | 18.2 | 18.1 | 20.9 | 94.9 | 98.3 | 98.3 |
| FLEURS, after   | 11.9 | 31.7 | 35.4 | 26.3 | 35.5 | 83.5 | 94.4 | 46.9 | 12.1 | 15.6 | 15.3 | 17.7 | 80.0 | 99.0 | 99.0 |
| Russian, before | 20.8 | 19.0 | 28.3 | - | - | 32.0 | - | 20.1 | - | 21.4 | - | - | - | 81.9 | - |
| Russian, after  | 17.4 | 18.8 | 26.7 | - | - | 29.8 | - | 20.1 | - | 17.7 | - | - | - | 86.3 | - |

What each change did (lecture / FLEURS WER, %, clean · laptop · reverb · babble 20 · opus 16).
Rows from "30 s cuts" to "q8" and "large-v3" build on the 60 s vote, 20 s cuts and greedy decoding;
the rows below them on the 30 s vote and 10 s cuts, and compare with that row:

| Setting | Lecture | FLEURS | Kept |
|---------|---------|--------|------|
| before | 30.3 · 86.2 · 97.6 · 30.7 · 85.9 | 14.6 · 31.9 · 38.5 · 45.4 · 18.2 | |
| language over 30 s instead of the first phrase | 30.7 · 31.8 · 61.8 · 32.6 · 31.8 | 13.8 · 32.1 · 37.9 · 45.4 · 17.2 | yes, 60 s + margin |
| + 20 s cuts | 23.6 · 25.9 · 51.2 · 27.5 · 25.7 | 13.1 · 32.4 · 35.6 · 41.7 · 17.3 | yes |
| 30 s cuts | 22.4 · 25.5 · 52.7 · 23.9 · 24.7 | 12.7 · 31.5 · 34.7 · 45.6 · 16.2 | no: lines twice as late for ~1 point |
| + beam 5 on finals | 22.0 · 26.1 · 50.3 · 23.9 · 25.3 | 11.4 · 31.8 · 35.4 · 37.4 · 15.5 | yes (GPU) |
| turbo q8_0 instead of q5_0 | 24.0 · 25.3 · 52.7 · 25.1 · 25.9 | 12.7 · 31.2 · 36.2 · 40.6 · 16.8 | no: ±1 point, 300 MB more |
| large-v3 q5_0 | 19.4 · 27.6 · 41.3 · 20.6 · 21.3 | 13.1 · 30.9 · 36.0 · 49.4 · 16.3 | no: the lecture reference is large-v3's own; on FLEURS no better, 3x slower |
| medium-q5_0 (30 s vote, 10 s cuts) | 37.7 · 42.9 · 69.6 · 43.6 · 39.0 | 21.5 · 37.2 · 49.1 · 52.1 · 23.5 | no: 7-8 points worse than turbo |
| beam 5 everywhere (10 s cuts) | 28.0 · 30.8 · 60.9 · 28.8 · 31.0 | 13.6 · 31.8 · 37.6 · 40.0 · 17.6 | finals only |
| temperature fallback 0.2 | 30.8 · 32.0 · 62.9 · 32.6 · 32.0 | 13.8 · 32.1 · 37.9 · 45.4 · 17.2 | no |
| `no_speech_thold` 1.0 (never drop) | same as 0.6 | same | no effect |
| energy detector instead of Silero | 32.6 · 35.7 · 93.2 · 43.4 · 34.5 | 32.7 · 34.5 · 46.3 · 47.6 · 31.5 | no |
| high-pass + AGC (ffmpeg `dynaudnorm`) | 29.8 · 28.7 · 64.5 · 31.4 · 32.6 | 11.5 · 32.1 · 23.2 · 46.8 · 11.4 | no: mixed, worse on the lecture's room |
| high-pass + FFT denoise (`afftdn`) + AGC | 30.3 · 33.0 · 64.7 · 34.2 · 34.2 | 12.8 · 32.2 · 27.1 · 42.4 · 12.8 | no: mixed, a denoiser to write |
| level normalised per utterance | 25.1 · 26.1 · 50.3 · 27.7 · 26.3 | 12.3 · 32.6 · 35.1 · 39.6 · 17.2 | no: ±1 point |

The big loss was the language. The lecture opens on a quoted line of verse; whisper scored it 0.47
Ukrainian / 0.43 Russian clean and Russian through a laptop microphone, Opus, clipping, a room or
fan noise, and the whole class then came out in Russian (WER 82-98%). Over the first minute of
speech, weighted by length, degraded Ukrainian scored at most 1.2 times as Russian as Ukrainian
(except `meet` 2.3 and `combo` 5.0), while degraded Russian scored 2.4 to 38 times more Russian,
hence the 1.5 margin. `meet`, `combo` and `worst` are lost with any setting, even with Ukrainian
forced (86% / 98%): past a point no decoding setting helps, only a better microphone does.
A lone "Дякую." came 1-5 times in every 7 min of clean speech and up to 66 times in noise, and
never in the references; "Субтитрувальниця Оля Шор" too.

Speed, after (Vulkan, turbo, partials every second, quiet GPU):

| Audio | RTF finals | RTF with partials | Final p95 | Partial p95 |
|-------|-----------:|------------------:|----------:|------------:|
| lecture 7 min, before | 0.021 | 0.13 | 0.21 s | 0.18 s |
| lecture 7 min, after | 0.045 | 0.21 | 0.93 s | 0.33 s |
| 90 s without pauses (en), before | 0.014 | 0.13 | 0.26 s | 0.26 s |
| 90 s without pauses (en), after | 0.024 | 0.23 | 0.75 s | 0.40 s |

The first minute also runs language detection on every call. The owner's settings had
`medium-q5_0` as the speech model; large-v3-turbo-q5_0 (the GPU default) is the better choice.
