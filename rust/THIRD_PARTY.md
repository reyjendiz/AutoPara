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

Tauri pulls in five MPL-2.0 crates (`cssparser`, `cssparser-macros`, `selectors`, `dtoa-short`,
`option-ext`), used unmodified; their source is on crates.io.

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
