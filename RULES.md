# Rules

## Code
- Rust 2024 edition, stable toolchain. `unsafe` is forbidden.
- No `unwrap()` or `expect()` outside tests. Return errors with `?`.
- Errors in `lecture-assist` use `crate::AssistError`. Do not add new error types.
- Log with `tracing::{info, warn, error}`. Never `println!` in library code.
- Never log or print API keys, audio samples or transcript text.
- Comments: English, short, and only for the *why* the code cannot show.
- No new dependencies unless the task names one. Use what `Cargo.toml` already has.
- Keep functions small and plain. No traits, generics or options the task does not ask for.
- Code must pass `cargo fmt` and `cargo clippy -- -D warnings`.

## Structure
- Rust lives under `rust/` (a cargo workspace). Run cargo commands from `rust/`.
- `rust/crates/autopara-core`: model, parser, database, scheduler, string table.
- `rust/crates/lecture-assist`: audio, VAD, speech recognition, LLM calls, notes.
- `rust/crates/autopara-app`: Tauri shell (`src-tauri/`) and TypeScript UI (`ui/src/`).
- Unit tests go at the bottom of the same file in `#[cfg(test)] mod tests { use super::*; ... }`.
- Only edit the files listed in the task. Module wiring (`mod x;`) is already done.

## Naming
- Types `UpperCamelCase`, functions and variables `snake_case`, constants `SCREAMING_SNAKE_CASE`.
- Names say what a thing is, in full words (`segment`, not `seg`).
- Test names describe the behaviour: `long_speech_is_cut_with_overlap`.

## User-visible text
- The app UI is Ukrainian. Every user-visible string is a key in
  `rust/crates/autopara-core/i18n/uk.json`; code refers to the key, never the literal text.

## Lecture Assist scope (never break)
- Text only, shown only on the user's own screen.
- Never write audio to disk. Never add text-to-speech, a virtual microphone or camera,
  anything that speaks or acts in the meeting, or anything that hides the app or its
  recording indicator from the user or from screen sharing.

## Do not
- Do not edit `rust/PROGRESS.md`, `rust/DECISIONS.md`, `PLAN.md` or `RULES.md`.
- Do not touch the Python app (`autopara/`, `tests/`, `android/`, `installer/`).
- Do not commit secrets, real timetables or model files.
