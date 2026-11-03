# Lecture Assist

An optional helper that listens to the class the user is already in, writes what is said as text
and keeps notes. It shows text on the user's own screen and nothing else.

## What it will never do

- Write audio to disk. Audio stays in memory for the few seconds it takes to recognise it.
- Speak, act in the meeting, or show up as a microphone or a camera.
- Hide itself or its recording indicator, from the user or from screen sharing. The tray icon
  gets a red dot while audio is captured, the assistant's panel shows a pulsing dot, and on a Mac the
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
               panel + database ◀── events ◀── engine.rs ◀── notes.rs / helpers.rs ◀── llm/
```

- **Audio** (`audio/`): on Windows, by default and while the class is open in AutoPara's tab,
  a process loopback of AutoPara itself (`app.rs`, `wasapi`): the class page plays from a WebView2
  child process, so only the class is heard, not music or videos elsewhere on the computer.
  Otherwise ("Весь звук комп’ютера", or a class not in the tab), WASAPI loopback of the default
  output device through `cpal`, following the default device when it changes and reconnecting when it drops. On macOS 13+,
  ScreenCaptureKit audio (`macos.rs`, untested on hardware); a chosen input device such as
  BlackHole is used instead when picked, or when the permission is missing. Resampled with
  `rubato`.
- **Recognition** (`stt.rs`): whisper.cpp through `whisper-rs`. The language is detected once
  among Ukrainian, Russian and English unless set. The class, teacher and glossary prime the
  spelling. A partial line every second, a final line per utterance.
- **Notes** (`notes.rs`, `llm/`): every 75 seconds the last 5 minutes of transcript go to the
  chat model, which returns topic, key points, definitions, deadlines and flagged items as JSON.
  The default chat model is built in (`llm/local.rs`): llama.cpp's `llama-server`, downloaded
  with the first chosen GGUF model and run on 127.0.0.1 with a random key, on the GPU through
  Vulkan or on the CPU. Any OpenAI-compatible server also works; requests are limited to 6 a
  minute. With fallback on, any other provider hands over to the built-in model when it is
  downloaded, and the built-in model hands over to a local Ollama.
- **Helpers** (`helpers.rs`): "recap the last 2 minutes", "explain simpler", "the topic in brief",
  a free question about the class, and a summary when listening stops.
- **Engine** (`engine.rs`): one worker thread; a panic inside becomes an error state, not a crash.
  States: `off → starting → listening ⇄ paused → stopping → off`, or `error`.

The app saves final lines, notes, terms and the summary in the `assist_*` tables, shows the
panel beside the class in the "Відкрита пара" tab (bringing AutoPara forward), relabels the tray item, and asks 10 minutes after the class ends whether to
keep listening (without an answer it stops after 2 minutes).

## Privacy model

- Recognition is local, and so is the default chat model. Nothing leaves the computer unless the
  user picks a cloud chat model.
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

Once the user has agreed to the assistant (the consent dialog), AutoPara downloads what listening
needs by itself, one file at a time, with a progress strip in the main window: the chosen speech
model, the voice detector and, with the built-in provider, its runtime and chosen model
(`assist::prepare`, at start, on consent and after the settings are saved). A model deleted while
it is still chosen comes back the same way; pick another one, or "Без нотаток", to keep it away.
The steps below choose other models or providers.

1. Settings → "Налаштувати помічника".
2. Download a speech model. The default is `small` (488 MB) on CPU builds; `large-v3-turbo-q5_0`
   on CUDA builds and `medium-q5_0` on Metal builds. Files are checked against their SHA-256 and
   kept in `models/` in the data folder. The small Silero model makes speech detection better.
3. For notes, answers and questions, download a built-in model ("Вбудована мовна модель"):
   Gemma 4 12B (7 GB, recommended for a graphics card with 8 GB or more), MamayLM 12B (7.3 GB,
   trained on Ukrainian text) or Gemma 4 E4B (5.2 GB, for weaker computers, also without a
   graphics card). The first download also fetches the llama.cpp runtime (38 MB). Windows only
   for now; on a Mac pick another provider. Or pick another provider: cloud presets need an API
   key; the field is write-only (leave it empty to keep the saved key). "Перевірити з’єднання"
   sends one short request. "Без нотаток" keeps only the transcript.
4. Start listening: the "Слухати пару" button in the top bar, "Слухати цю пару" in a class's
   menu, the tray, or the shortcut. With "Слухати автоматично, коли AutoPara відкриває пару" on
   (off by default), it starts by itself whenever a class is opened on time or joined by hand
   today (`assist::listen_on_open`); consent is still required and the indicators are the same.

A class in AutoPara's tab is heard on its own. A class elsewhere (a browser tab, the Zoom app) is
heard through the computer's sound output, together with everything else that plays, as long as
it plays through the captured device and the tab is not muted.

## Shortcut and panel

- `Ctrl+Alt+L` starts and stops listening from anywhere. It can be changed in the settings; a
  combination another program holds is reported as taken and the old one is kept.
- The assistant is a panel on the right of the "Відкрита пара" tab, beside the class page, which
  narrows to make room. It appears (and the tab comes forward) when listening starts, folds to
  its header, has a font size slider, and can be closed once listening has stopped.
- "Вимкнути ШІ" in the panel stops the language model for the rest of the class (and the
  built-in model's server with it, freeing the graphics card); the transcript goes on.

## Troubleshooting

| Symptom | What to check |
|---------|---------------|
| "Модель розпізнавання недоступна" | Download the model in the assistant settings. |
| No text appears | Is sound actually playing through the default output? Pick the device by hand. On a Mac, grant Screen Recording or select BlackHole. |
| Text is late | Use a smaller model, or a GPU build; see `ASSIST_BENCH.md`. |
| Notes never appear | No chat model selected, a wrong key, or the server is down: use "Перевірити з’єднання". |
| "Забагато запитів" | The provider's rate limit; notes resume on their own. |
| The shortcut does nothing | Another program holds it: the settings say so. Choose another. |

Errors and state changes are in the log (`logs/` in the data folder), without any text heard.
