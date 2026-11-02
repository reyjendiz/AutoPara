# Third-party components

Direct dependencies with their licences. Transitive Rust dependencies are checked by `cargo deny
check` (`deny.toml`); exceptions are explained in `DECISIONS.md`.

## Rust (shipped)

| Crate | Version | Licence | Used for |
|---|---|---|---|
| tauri | 2.12.5 | MIT OR Apache-2.0 | window, tray, IPC |
| tauri-build | 2.7.4 | MIT OR Apache-2.0 | build script |
| serde | 1.0.229 | MIT OR Apache-2.0 | serialisation |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | JSON (string table, IPC) |
| ts-rs | 12.0.1 | MIT | TypeScript types from Rust types |
| caseless | 0.2.2 | MIT | Unicode case folding, as Python's `casefold` |
| chrono | 0.4.45 | MIT OR Apache-2.0 | dates and times |
| quick-xml | 0.42.0 | MIT | reading the `.docx` XML |
| regex | 1.13.1 | MIT OR Apache-2.0 | times and URLs in cell text |
| rusqlite (bundled SQLite) | 0.41.0 | MIT (SQLite: public domain) | the database |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | error types |
| tokio | 1.54.1 | MIT | the scheduler's background task |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | NFC normalisation |
| zip (deflate only) | 9.1.2 | MIT | reading the `.docx` archive |
| tauri-plugin-dialog | 2.8.3 | Apache-2.0 OR MIT | the file picker |
| tauri-plugin-notification | 2.7.0 | Apache-2.0 OR MIT | reminders and notices |
| tauri-plugin-opener | 2.7.2 | Apache-2.0 OR MIT | opening meeting links |
| tauri-plugin-single-instance | 2.5.4 | Apache-2.0 OR MIT | one running copy |
| tauri-plugin-window-state | 2.5.0 | Apache-2.0 OR MIT | window size and position |
| tauri-plugin-updater (no `zip` feature) | 2.14.0 | Apache-2.0 OR MIT | signed updates from GitHub Releases |
| tracing / tracing-subscriber / tracing-appender | 0.1.44 / 0.3.23 / 0.2.5 | MIT | the log file |
| windows-registry (Windows only) | 0.100.0 | MIT OR Apache-2.0 | the autostart entry |
| cpal | 0.18.2 | Apache-2.0 | Lecture Assist: WASAPI loopback and input capture |
| rubato | 5.0.1 | MIT | Lecture Assist: resampling to 16 kHz |
| whisper-rs / whisper-rs-sys | 0.16.0 / 0.15.0 | Unlicense (bundled whisper.cpp and ggml: MIT) | Lecture Assist: speech recognition and Silero VAD |
| reqwest (rustls, no default features) | 0.13.5 | MIT OR Apache-2.0 | Lecture Assist: chat providers |
| rustls (ring) | 0.23.46 | Apache-2.0 OR ISC OR MIT | TLS for chat providers |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | streamed chat replies |
| hound | 3.5.1 | Apache-2.0 | reading wav fixtures for tests and `bench-stt` (never writes audio) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | checking downloaded model files |
| keyring (Windows and macOS stores only) | 3.6.3 | MIT OR Apache-2.0 | chat provider API keys in the OS keychain |
| tauri-plugin-global-shortcut (global-hotkey 0.8.0) | 2.4.1 | Apache-2.0 OR MIT | Lecture Assist: the `Ctrl+Alt+L` shortcut |
| screencapturekit (macOS only; apple-cf 0.11.7, apple-metal 0.10.4) | 11.1.1 | MIT OR Apache-2.0 | Lecture Assist: system audio on macOS |

Tauri pulls in five MPL-2.0 crates (`cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`,
`option-ext`), used unmodified; their source is on crates.io.

## Rust (tests only)

| Crate | Version | Licence |
|---|---|---|
| chrono-tz | 0.10.4 | MIT OR Apache-2.0 |
| proptest | 1.12.0 | MIT OR Apache-2.0 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| wiremock | 0.6.5 | MIT OR Apache-2.0 |

## Frontend (shipped)

| Package | Version | Licence |
|---|---|---|
| @tauri-apps/api | 2.12.4 | Apache-2.0 OR MIT |

## Frontend tooling (build and test only, not shipped)

| Package | Version | Licence |
|---|---|---|
| @tauri-apps/cli | 2.12.4 | Apache-2.0 OR MIT |
| vite | 8.3.5 | MIT |
| typescript | 6.0.3 | Apache-2.0 |
| vitest | 5.0.5 | MIT |
| eslint | 10.13.0 | MIT |
| @eslint/js | 10.0.1 | MIT |
| typescript-eslint | 8.72.1 | MIT |

Their transitive packages are MIT, Apache-2.0, BSD-2/3-Clause, ISC and BlueOak-1.0.0, except
`lightningcss` (MPL-2.0, Vite's CSS processor, build time only).

## Developer tool `tools/localdev` (not shipped)

| Crate | Version | Licence |
|---|---|---|
| ureq | 3.5.0 | MIT OR Apache-2.0 |
| toml | 1.1.8 | MIT OR Apache-2.0 |

## Models (downloaded on demand, never bundled)

| Model | Licence |
|---|---|
| whisper ggml models (`ggerganov/whisper.cpp` on Hugging Face) | MIT |
| Silero VAD v5, ggml conversion | MIT |
| llama.cpp `llama-server`, release b12052, Windows Vulkan build (the built-in chat model's runtime) | MIT |
| Gemma 4 12B and E4B, Google's QAT q4_0 GGUF | Apache-2.0 |
| MamayLM Gemma 3 12B v2.0, INSAIT's Q4_K_M GGUF | Gemma Terms of Use |

## Assets

| Asset | Licence |
|---|---|
| App icons (`src-tauri/icons`) | rendered from the project's own `autopara/ui/assets/logo.png` |

## Installer (Windows, shipped inside the Setup.exe)

| Component | Licence |
|---|---|
| NSIS template `src-tauri/installer/installer.nsi` | adapted from tauri-bundler 2.10.4's own template, MIT OR Apache-2.0 |
| NSIS and its plugins (downloaded by the Tauri CLI at build time) | zlib/libpng (its LZMA decoder: CPL-1.0 with NSIS's exception for installers) |
| `nsis_tauri_utils` plugin | MIT OR Apache-2.0 |
| Microsoft Edge WebView2 Evergreen bootstrapper | Microsoft WebView2 distribution terms (redistribution allowed) |
