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

Tauri pulls in five MPL-2.0 crates (`cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`,
`option-ext`), used unmodified; their source is on crates.io.

## Rust (tests only)

| Crate | Version | Licence |
|---|---|---|
| chrono-tz | 0.10.4 | MIT OR Apache-2.0 |
| proptest | 1.12.0 | MIT OR Apache-2.0 |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |

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
