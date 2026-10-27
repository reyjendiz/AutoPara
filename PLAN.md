# Plan: Phase 7, Lecture Assist

Broken out of `AUTOPARA_RUST_PLAN.md`, Phase 7 (steps 7.1–7.14). Every task follows `RULES.md`.
Run `verify` commands from `rust/`. `(lead)` tasks are architectural or span many files and are
done by hand. `(worker)` tasks were meant for `tools/localdev`;
the local model proved too slow on them, so they are done by hand as well.

Task format: `edit` lists the files the task may change, `read` lists files given as context only.

Run the worker from the repository root:
`cargo run --release --manifest-path tools/localdev/Cargo.toml -- [TASK_ID]`.

## Setup

- [x] **T00** Lecture Assist groundwork (lead)
  - edit: `rust/crates/lecture-assist/**`, `rust/.cargo/config.toml`, `rust/deny.toml`
  - result: dependencies; shared types in `lib.rs` (state, events, context, errors); `vad.rs`
    (energy VAD, Silero through whisper.cpp, segmenter) and `text.rs` (phantom and repetition
    filters, overlap removal, whisper prompt, question/task flags) with tests; an empty,
    documented file for every later module, already wired with `mod`, so worker tasks touch
    one file. `Unlicense` (whisper-rs) allowed in `deny.toml`.
  - verify: `cargo test -p lecture-assist && cargo clippy -p lecture-assist --all-targets -- -D warnings`

- [x] **T01** `tools/localdev` bridge CLI (lead)
  - edit: `tools/localdev/**`, `localdev.toml`
  - result: takes the next unchecked worker task, prompts the local model, applies files, runs
    `verify`, retries up to 3 times, ticks and commits on success, reverts on failure,
    prints a short report.
  - verify: `cargo build --manifest-path ../tools/localdev/Cargo.toml`

## LLM provider layer (7.6)

- [x] **T02** SSE parser (worker)
  - edit: `rust/crates/lecture-assist/src/llm/sse.rs`
  - result: `pub struct SseParser` with `pub fn push(&mut self, bytes: &[u8]) -> Vec<String>`
    returning the payload of every complete `data:` line (bytes may split lines and UTF-8
    characters anywhere; ignore comments, empty lines and `[DONE]`). Tests for split input.
  - verify: `cargo test -p lecture-assist llm::sse`

- [x] **T03** Requests-per-minute guard (worker)
  - edit: `rust/crates/lecture-assist/src/llm/rate.rs`
  - result: `pub struct RateGuard { limit, sent: VecDeque<Instant> }`, `new(limit: u32)`,
    `try_acquire(&mut self, now: Instant) -> bool` allowing at most `limit` calls in any
    60-second window. Tests with fixed instants.
  - verify: `cargo test -p lecture-assist llm::rate`

- [x] **T04** `ChatProvider` trait and message types (lead)
  - edit: `rust/crates/lecture-assist/src/llm/mod.rs`
  - result: `Message`, `Role`, `ChatRequest`, `ChatProvider` (streaming, boxed), error mapping
    (timeout, network, HTTP 429/5xx → `ChatUnavailable`, so the chain falls back).
  - verify: `cargo test -p lecture-assist llm`

- [x] **T05** OpenAI-compatible streaming client (worker)
  - edit: `rust/crates/lecture-assist/src/llm/openai.rs`
  - read: `rust/crates/lecture-assist/src/llm/mod.rs`, `rust/crates/lecture-assist/src/llm/sse.rs`
  - result: `OpenAiCompatible { base_url, model, api_key: Option<String>, temperature, timeout }`
    implementing `ChatProvider`: POST `{base_url}/chat/completions` with `stream: true`,
    yields `choices[0].delta.content` pieces. Tests against `wiremock`: streamed reply,
    429 → `ChatUnavailable`, 401 → `Chat`.
  - verify: `cargo test -p lecture-assist llm::openai`

- [x] **T06** Provider chain with fallback (worker)
  - edit: `rust/crates/lecture-assist/src/llm/chain.rs`
  - read: `rust/crates/lecture-assist/src/llm/mod.rs`
  - result: `ProviderChain(Vec<Arc<dyn ChatProvider>>)` implementing `ChatProvider`: tries
    each in order, moves on only on `ChatUnavailable` before the first piece arrives.
    Tests with two fake providers.
  - verify: `cargo test -p lecture-assist llm::chain`

- [x] **T07** Provider presets (worker)
  - edit: `rust/crates/lecture-assist/src/llm/presets.rs`
  - result: `pub struct Preset { id, base_url, model, local: bool }` and `pub const PRESETS`:
    DeepSeek, Qwen (DashScope compatible mode), Ollama, LM Studio, llama.cpp server.
    Test that ids are unique and local presets point at 127.0.0.1.
  - verify: `cargo test -p lecture-assist llm::presets`

## Notes and helpers (7.7)

- [x] **T08** Transcript window (worker)
  - edit: `rust/crates/lecture-assist/src/notes.rs` (the `Transcript` part)
  - read: `rust/crates/lecture-assist/src/lib.rs`
  - result: `Transcript { segments: Vec<Segment>, summary: String }` with `push`,
    `window(minutes) -> String` (text of segments ending in the last N minutes),
    `since(ms) -> String`, `last_text() -> &str`. Tests.
  - verify: `cargo test -p lecture-assist notes`

- [x] **T09** Notes prompt and reply parsing (worker)
  - edit: `rust/crates/lecture-assist/src/notes.rs` (the `Notes` part)
  - result: `Notes { topic, key_points, definitions, deadlines, flagged }`,
    `notes_messages(context, transcript) -> Vec<Message>` asking for strict JSON
    (topic, key_points, definitions, deadlines, terms, summary) in the class language,
    lenient `parse_reply(&str) -> Option<NotesReply>` (finds the first `{…}` block),
    `Notes::to_markdown()`. Tests, including a reply wrapped in prose.
  - verify: `cargo test -p lecture-assist notes`

- [x] **T10** Helper prompts (worker)
  - edit: `rust/crates/lecture-assist/src/helpers.rs`
  - read: `rust/crates/lecture-assist/src/lib.rs`, `rust/crates/lecture-assist/src/notes.rs`
  - result: `helper_messages(kind: AskKind, context, transcript, text) -> Vec<Message>` for
    recap (last 2 minutes), simplify, topic, free question, end-of-class summary in Markdown.
    Prompts tell the model to explain to the student, never to write answers for them.
  - verify: `cargo test -p lecture-assist helpers`

## Audio (7.2, 7.3)

- [x] **T11** Resampling to 16 kHz mono (worker)
  - edit: `rust/crates/lecture-assist/src/audio/resample.rs`
  - result: `To16k::new(rate: u32, channels: u16)`, `process(&mut self, interleaved: &[f32])
    -> Vec<f32>` using `rubato` (downmix first, keep leftovers between calls). Test: 1 s of
    48 kHz stereo gives 16000 ± 64 samples.
  - verify: `cargo test -p lecture-assist audio::resample`

- [x] **T12** Audio sources: trait, WASAPI loopback, device list, reconnect, level (lead)
  - edit: `rust/crates/lecture-assist/src/audio/mod.rs`, `rust/crates/lecture-assist/src/audio/capture.rs`
  - result: `AudioSource` trait; `cpal` capture of an output device's loopback on Windows and an
    input device elsewhere; device list; reconnect on device change; pause stops the stream.
  - verify: `cargo test -p lecture-assist audio`

- [x] **T13** Fake audio source for tests (worker)
  - edit: `rust/crates/lecture-assist/src/audio/fake.rs`
  - read: `rust/crates/lecture-assist/src/audio/mod.rs`
  - result: `FakeSource::new(samples: Vec<f32>, chunk: usize)` implementing `AudioSource`;
    `FakeSource::from_wav(path)` via `hound` (16 kHz mono). Test.
  - verify: `cargo test -p lecture-assist audio::fake`

- [ ] **T14** macOS ScreenCaptureKit source (lead, untested on hardware)
  - edit: `rust/crates/lecture-assist/src/audio/macos.rs`
  - result: audio-only SCK stream (macOS 13+) with permission check; falls back to an input
    device (BlackHole). Compiled only on macOS, so checked by CI.
  - verify: CI macOS job

## Speech recognition (7.5)

- [ ] **T15** Model catalogue and manager (worker)
  - edit: `rust/crates/lecture-assist/src/models.rs`
  - result: catalogue (id, file, URL, size, SHA-256; given in the task) for `large-v3-turbo-q5_0`,
    `medium-q5_0`, `small`, Silero VAD; `path`, `installed`, `delete`, and `download` with a
    progress callback that writes to a `.part` file, checks SHA-256 and renames.
    Test against `wiremock` (good hash, bad hash leaves nothing behind).
  - verify: `cargo test -p lecture-assist models`

- [x] **T16** `Recognizer` trait and whisper backend (lead)
  - edit: `rust/crates/lecture-assist/src/stt.rs`
  - result: load model (GPU when built with `cuda`/`metal`), language auto/fixed, initial
    prompt, no-speech threshold, segments with timestamps.
  - verify: `cargo test -p lecture-assist stt`

## Engine (7.1)

- [x] **T17** `AssistEngine` and `Handle` (lead)
  - edit: `rust/crates/lecture-assist/src/engine.rs`
  - result: worker thread with panic isolation; state machine; partial decode about once a
    second, final on silence; flags; notes timer; helpers; end-of-class summary; event stream.
  - verify: `cargo test -p lecture-assist engine`

- [x] **T18** End-to-end engine test (worker)
  - edit: `rust/crates/lecture-assist/tests/engine.rs`
  - read: `rust/crates/lecture-assist/src/engine.rs`, `rust/crates/lecture-assist/src/audio/fake.rs`
  - result: fake source + scripted recognizer + mock chat provider; asserts transcript,
    flags, notes and the state sequence `Starting → Listening → Stopping → Off`.
  - verify: `cargo test -p lecture-assist --test engine`

## Storage (7.10)

- [ ] **T19** Assist tables migration (worker)
  - edit: `rust/crates/autopara-core/src/db.rs`
  - result: a new entry in `MIGRATIONS` adding `assist_sessions`, `assist_segments`,
    `assist_notes`, plus `Storage` methods to create a session, add segments, save notes and
    summary, list sessions. Tests.
  - verify: `cargo test -p autopara-core db`
  - format: `diff`

- [ ] **T20** Retention and "delete everything" (worker)
  - edit: `rust/crates/autopara-core/src/db.rs`
  - result: `purge_assist_older_than(days, now)` and `delete_all_assist()`. Tests.
  - verify: `cargo test -p autopara-core db`
  - format: `diff`

## App integration (7.8, 7.9, 7.11)

- [ ] **T21** Ukrainian strings for Lecture Assist (worker)
  - edit: `rust/crates/autopara-core/i18n/uk.json`
  - result: the keys listed in the task (consent text, tray items, states, errors, settings,
    helper buttons, end-of-class question).
  - verify: `cargo test -p autopara-core i18n`

- [ ] **T22** App wiring: assist state, commands, events, storage, tray, keychain (lead)
  - edit: `rust/crates/autopara-app/src-tauri/src/assist.rs`, `src/lib.rs`, `src/tray.rs`, `Cargo.toml`
  - result: start/pause/resume/stop/ask commands, consent gate, events to both windows, text
    saved to the database, tray item and icon change, API key in the OS keychain, isolation
    from the scheduler, end-of-class prompt with 10-minute grace.
  - verify: `cargo test -p autopara-app`

- [ ] **T23** Global hotkey with conflict detection (worker)
  - edit: `rust/crates/autopara-app/src-tauri/src/hotkey.rs`
  - read: `rust/crates/autopara-app/src-tauri/src/assist.rs`
  - result: register `Ctrl+Alt+L` (configurable) with `tauri-plugin-global-shortcut`; a taken
    shortcut reports a conflict instead of failing.
  - verify: `cargo test -p autopara-app hotkey`

- [ ] **T24** Overlay window shell (lead)
  - edit: `rust/crates/autopara-app/src-tauri/tauri.conf.json`, `rust/crates/autopara-app/src-tauri/capabilities/overlay.json`, `rust/crates/autopara-app/vite.config.ts`, `rust/crates/autopara-app/overlay.html`
  - result: second window `overlay`: transparent, frameless, always on top, not in the taskbar,
    resizable, position remembered per monitor.
  - verify: `cargo xtask check`

- [ ] **T25** Overlay UI (worker)
  - edit: `rust/crates/autopara-app/ui/src/overlay/overlay.ts`, `rust/crates/autopara-app/ui/src/overlay/overlay.css`
  - read: `rust/crates/autopara-app/ui/src/i18n.ts`, `rust/crates/autopara-app/ui/src/dom.ts`
  - result: recording dot with elapsed time and class title, transcript tail, notes, terms,
    flagged items, helper buttons and text field, opacity and font size, collapse to a pill,
    click-through toggle; readable on any background.
  - verify: `cd crates/autopara-app && npm run typecheck && npm run lint && npm test`

- [ ] **T26** Consent dialog and assist settings (worker)
  - edit: `rust/crates/autopara-app/ui/src/ui/dialogs/assist.ts`
  - read: `rust/crates/autopara-app/ui/src/ui/dialogs/settings.ts`, `rust/crates/autopara-app/ui/src/backend.ts`
  - result: first-run consent (what is captured, where it is processed, user's responsibility);
    settings: audio source, model download/delete with sizes, language, LLM preset, write-only
    API key, test connection, fallback, hotkey, overlay, retention, revoke consent.
  - verify: `cd crates/autopara-app && npm run typecheck && npm run lint && npm test`

- [ ] **T27** Listen button by the "now" pill and "Слухати цю пару" menu entry (worker)
  - edit: `rust/crates/autopara-app/ui/src/ui/nav.ts`, `rust/crates/autopara-app/ui/src/ui/popups.ts`
  - result: both call the start command with the class; the pill shows the listening state.
  - verify: `cd crates/autopara-app && npm run typecheck && npm run lint && npm test`

## Bench, docs, gate (7.12–7.14)

- [ ] **T28** `cargo xtask bench-stt <wav>` (lead)
  - edit: `rust/xtask/src/main.rs`, `rust/xtask/Cargo.toml`
  - result: prints real-time factor, partial and final latency, memory; results in
    `docs/ASSIST_BENCH.md`.
  - verify: `cargo xtask bench-stt <fixture.wav>`

- [ ] **T29** `docs/ASSIST.md` (lead)
  - result: architecture, privacy model, model and key setup, hotkeys, troubleshooting.

- [ ] **T30** Decisions, licences, gate, progress (lead)
  - edit: `rust/DECISIONS.md`, `rust/THIRD_PARTY.md`, `rust/PROGRESS.md`
  - verify: `cargo xtask check` and `cargo deny check`
