# Lecture Assist

An optional helper that listens to the class the user is already in, writes what is said as text
and keeps notes. It shows text on the user's own screen and nothing else.

## What it will never do

- Write audio to disk. Audio stays in memory for the few seconds it takes to recognise it.
- Speak, act in the meeting, or show up as a microphone or a camera.
- Hide itself or its recording indicator, from the user or from screen sharing. The tray icon
  gets a red dot while audio is captured, the overlay shows a pulsing dot, and on a Mac the
  system's own screen-recording indicator is on.
- Answer the questions and tasks it hears. They are flagged ("Питання / Завдання") for the
  student to deal with.

## Architecture

The `lecture-assist` crate does the work; the app only wires it up (`src-tauri/src/assist.rs`).
It runs beside the scheduler and shares nothing with it but the database, so an assistant
failure never stops a class from opening.

```
audio source ─▶ 16 kHz mono ─▶ voice detector ─▶ segmenter ─▶ whisper ─▶ text clean-up
(capture.rs,                    (vad.rs: Silero   (≤ 28 s,      (stt.rs)   (text.rs: phantoms,
 macos.rs, fake.rs)              or energy)        0.5 s overlap)           repeats, flags)
                                                                              │
             overlay + database ◀── events ◀── engine.rs ◀── notes.rs / helpers.rs ◀── llm/
```

- **Audio** (`audio/`): on Windows, WASAPI loopback of the default output device through `cpal`,
  following the default device when it changes and reconnecting when it drops. On macOS 13+,
  ScreenCaptureKit audio (`macos.rs`, untested on hardware); a chosen input device such as
  BlackHole is used instead when picked, or when the permission is missing. Resampled with
  `rubato`.
- **Recognition** (`stt.rs`): whisper.cpp through `whisper-rs`. The language is detected once
  among Ukrainian, Russian and English unless set. The class, teacher and glossary prime the
  spelling. A partial line every second, a final line per utterance.
- **Notes** (`notes.rs`, `llm/`): every 75 seconds the last 5 minutes of transcript go to the
  chat model, which returns topic, key points, definitions, deadlines and flagged items as JSON.
  Any OpenAI-compatible server works; requests are limited to 6 a minute. With fallback on, a
  failing cloud provider hands over to a local Ollama.
- **Helpers** (`helpers.rs`): "recap the last 2 minutes", "explain simpler", "the topic in brief",
  a free question about the class, and a summary when listening stops.
- **Engine** (`engine.rs`): one worker thread; a panic inside becomes an error state, not a crash.
  States: `off → starting → listening ⇄ paused → stopping → off`, or `error`.

The app saves final lines, notes, terms and the summary in the `assist_*` tables, shows the
overlay window, relabels the tray item, and asks 10 minutes after the class ends whether to
keep listening (without an answer it stops after 2 minutes).

## Privacy model

- Recognition is local. Nothing leaves the computer unless the user picks a cloud chat model.
- With a cloud model (DeepSeek, Qwen, or a custom server), short transcript windows and the
  user's questions go to that provider. Local presets (Ollama, LM Studio, llama.cpp) keep
  everything on the machine.
- API keys are kept in the OS keychain (service "AutoPara Lecture Assist", one entry per
  preset), never in the database or the log, and never sent back to the interface.
- Transcripts and notes are kept 30 days by default (1–3650) and purged at start-up. "Видалити
  всі дані помічника" deletes them all with SQLite `secure_delete` on.
- The log never contains audio, transcript text or keys.
- The first start asks for consent: what is captured, where it is processed, how long it is
  kept, and that following the university's rules on recording is the student's responsibility.
  Consent can be revoked in the settings, which also stops listening.

## Setting up

1. Settings → "Налаштувати помічника".
2. Download a speech model. The default is `small` (488 MB) on CPU builds; `large-v3-turbo-q5_0`
   on CUDA builds and `medium-q5_0` on Metal builds. Files are checked against their SHA-256 and
   kept in `models/` in the data folder. The small Silero model makes speech detection better.
3. Optionally pick a chat model for notes. Cloud presets need an API key; the field is
   write-only (leave it empty to keep the saved key). "Перевірити з’єднання" sends one short
   request.
4. Start listening: the "Слухати пару" button in the top bar, "Слухати цю пару" in a class's
   menu, the tray, or the shortcut.

## Shortcut and overlay

- `Ctrl+Alt+L` starts and stops listening from anywhere. It can be changed in the settings; a
  combination another program holds is reported as taken and the old one is kept.
- The overlay is a small always-on-top window, outside the taskbar, that remembers where it
  was. It has opacity and font size sliders, collapses to a pill, and has a click-through
  switch. A click-through window cannot be clicked, so opening AutoPara from the tray turns
  click-through off again.

## Troubleshooting

| Symptom | What to check |
|---------|---------------|
| "Модель розпізнавання недоступна" | Download the model in the assistant settings. |
| No text appears | Is sound actually playing through the default output? Pick the device by hand. On a Mac, grant Screen Recording or select BlackHole. |
| Text is late | Use a smaller model, or a GPU build; see `ASSIST_BENCH.md`. |
| Notes never appear | No chat model selected, a wrong key, or the server is down: use "Перевірити з’єднання". |
| "Забагато запитів" | The provider's rate limit; notes resume on their own. |
| The shortcut does nothing | Another program holds it: the settings say so. Choose another. |
| The overlay ignores clicks | Click-through is on: open AutoPara from the tray. |

Errors and state changes are in the log (`logs/` in the data folder), without any text heard.
