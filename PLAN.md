# Rust migration plan: AutoPara-orig (Python)

1 packages, 42 source files (8400 lines), 24 test files, 0 UI files, grouped into 6 units.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 3 | 3 | 0 | 0 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `autopara/` | 232 | 71 | 90 | 3 | PyO3 | Dynamic typing (2); Exceptions (1); Number semantics (1) |
| 2 | `tools/` | 252 | 67 | 73 | 0 | PyO3 | Hash iteration order (3); String offsets and slicing (2); Class inheritance (2) |
| 3 | `autopara/importer/` | 687 | 64 | 59 | 8 | PyO3 | String offsets and slicing (8); Regexes built at runtime (1); Hash iteration order (1) |

### `autopara/`

232 lines in 3 files, wave 1, priority 71, readiness 90, bridge **PyO3**.

- 1 PyPI dependencies not in the crate table: check them by hand.
- Import cycle with autopara/ui/, autopara/core/: port them together.
- 3 test files import it directly.
- Dynamic typing x2: `autopara/app.py:196`, `autopara/app.py:200`
- Exceptions x1: `autopara/app.py:51`
- Number semantics x1: `autopara/app.py:109`

### `tools/`

252 lines in 2 files, wave 2, priority 67, readiness 73, bridge **PyO3**.

- 1 PyPI dependencies not in the crate table: check them by hand.
- Hash iteration order x3: `tools/bump_version.py:34`, `tools/bump_version.py:35`, `tools/bump_version.py:105`
- String offsets and slicing x2: `tools/bump_version.py:126`, `tools/bump_version.py:163`
- Class inheritance x2: `tools/screenshots.py:32`, `tools/screenshots.py:37`
- Exceptions x4: `tools/bump_version.py:40`, `tools/bump_version.py:67`, `tools/bump_version.py:75`
- Number semantics x1: `tools/bump_version.py:105`

### `autopara/importer/`

687 lines in 4 files, wave 0, priority 64, readiness 59, bridge **PyO3**.

- Leaf: imports no other unit.
- 8 test files import it directly.
- String offsets and slicing x8: `autopara/importer/docx_reader.py:130`, `autopara/importer/docx_reader.py:132`, `autopara/importer/docx_reader.py:138`
- Regexes built at runtime x1: `autopara/importer/normalize.py:85`
- Hash iteration order x1: `autopara/importer/normalize.py:269`
- Division and modulo x3: `autopara/importer/normalize.py:142`, `autopara/importer/normalize.py:239`, `autopara/importer/normalize.py:239`
- Number semantics x12: `autopara/importer/docx_reader.py:36`, `autopara/importer/docx_reader.py:60`, `autopara/importer/docx_reader.py:129`
- Exceptions x2: `autopara/importer/docx_reader.py:90`, `autopara/importer/docx_reader.py:134`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. AutoPara-orig (`.`, portable, 24 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `autopara/importer/` | ready | 687 | 59 | PyO3 | String offsets and slicing (8); Regexes built at runtime (1) |
| 1 | `autopara/` | ready | 232 | 90 | PyO3 | Dynamic typing (2); Exceptions (1) |
| 1 | `autopara/core/` | caution | 2273 | 3 | PyO3 | Hash iteration order (9); Dynamic typing (12) |
| 1 | `autopara/ui/` | caution | 4852 | 0 | PyO3 | String offsets and slicing (9); Hash iteration order (2) |
| 2 | `build_mac.py` | caution | 104 | 98 | PyO3 | Division and modulo (2) |
| 2 | `tools/` | ready | 252 | 73 | PyO3 | Hash iteration order (3); String offsets and slicing (2) |

## PyPI dependencies

| Package | Units | Rust | Fit | Note |
| --- | ---: | --- | --- | --- |
| `pyside6` | 4 | ? | unknown |  |
| `certifi` | 1 | ? | unknown |  |

## Risk guide

- **String offsets and slicing** (High): JS indexes strings in UTF-16 code units, Rust in UTF-8 bytes. Offsets that cross the boundary must be converted, or non-ASCII text gets wrong ranges silently.
- **Regexes built at runtime** (High): Patterns come from data or config in JS syntax. Rust regex syntax differs, so user-visible behavior can change. Pin it with tests on real patterns.
- **Dynamic typing** (Medium): Values whose shape is checked at runtime need an explicit Rust enum or `serde_json::Value`.
- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Class inheritance** (Medium): Class hierarchies become traits plus composition; `instanceof` checks become enums.
- **Number semantics** (Low): JS numbers are f64; bitwise ops truncate to i32; parseInt is lenient. Pick Rust integer types deliberately. Printing differs: `${n}` gives `1e+21`, `1e-7` and `0` for -0 where Rust's `{}` does not, so use `js::number_to_string`. `Math.round` rounds halves up (-2.5 gives -2), Rust's `round` away from zero: use `js::math_round`. `toFixed` rounds exact halves up ((2.5).toFixed(0) is "3"), Rust's `{:.0}` to even.
- **Division and modulo** (Medium): Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined.
- **Hash iteration order** (High): Set and map iteration order comes from the source language's hash function. A port cannot reproduce it: sort before output, or make the order part of the contract.
- **Concurrency** (Medium): Threads, goroutines and locks need a design in Rust (channels, Send/Sync), not a line-by-line translation; keep concurrency in the caller where possible.

## Whole app

Shape: **server**.

| Entry point | Kind | Found from |
| --- | --- | --- |
| `autopara/__main__.py` | cli | `if __name__ == "__main__"` at autopara/__main__.py:13 |
| `autopara/app.py` | server | `.listen(` at autopara/app.py:154 |
| `build_mac.py` | cli | `if __name__ == "__main__"` at build_mac.py:122 |
| `tools/bump_version.py` | cli | `if __name__ == "__main__"` at tools/bump_version.py:183 |
| `tools/screenshots.py` | cli | `if __name__ == "__main__"` at tools/screenshots.py:123 |

### Every dependency

| Dependency | Declared | Used by | Rust | Fit | Note |
| --- | --- | --- | --- | --- | --- |
| `pyside6` | yes | 4 | ? | unknown |  |
| `certifi` | yes | 1 | ? | unknown |  |

### Pieces, in port order

- wave 0: `autopara/importer/` (687 lines)
- wave 1: `autopara/`, `autopara/core/`, `autopara/ui/`, tangled: one piece (7357 lines)
- wave 2: `build_mac.py` (104 lines)
- wave 2: `tools/` (252 lines)

### Decisions for the user

**D1** (shape): The app is a server, entered through autopara/__main__.py, autopara/app.py, build_mac.py, tools/bump_version.py, tools/screenshots.py. Port every entry point, in that shape?

> Yes: an axum front door (`app front-door`) takes the server over first

**D2** (dependency pyside6): Which crate replaces pyside6 (used by 4 units)?

> Research it against primary sources into rust/research/pyside6.md before porting what uses it

**D3** (dependency certifi): Which crate replaces certifi (used by 1 unit)?

> Research it against primary sources into rust/research/certifi.md before porting what uses it

**D4** (tangled units): autopara/, autopara/core/, autopara/ui/ import each other (7357 lines). Port them as one piece?

> Yes: one piece in wave 1, no glue between its parts

