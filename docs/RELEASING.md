# Releasing AutoPara

How a release is cut, and how to pick the version number. The hands-on checks before and after are
`docs/RELEASE_CHECKLIST.md`.

## How a release actually happens here

`.github/workflows/rust-release.yml` runs on every push to `main` that touches `rust/` (and on
manual dispatch). What it does depends on one number:

- It reads the version with `cargo metadata` — `[workspace.package] version` in `rust/Cargo.toml`,
  which every crate inherits and Tauri takes for the installer, the executable's version info and
  Add/Remove Programs.
- If `v<version>` already has a release, it stops: nothing is built or published.
- Otherwise it builds Windows, Apple Silicon, Intel and Android in parallel, signs the updater files
  with `TAURI_SIGNING_PRIVATE_KEY`, and **only when all four succeeded** creates `v<version>` with
  `AutoPara-<version>-Setup.exe`, `AutoPara-<version>.dmg`, `AutoPara-Intel-<version>.dmg`,
  `AutoPara-<version>.apk`, the two `.app.tar.gz` update archives, their `.sig` files and
  `latest.json`.

So the number is the whole switch. Two consequences worth holding onto:

- **A version whose release exists is spent.** You cannot re-cut `v2.0.0`; you can only go forward.
  That is why the number gets confirmed with the owner before it is written.
- **Publishing a release is shipping an update.** Every 2.x copy reads
  `releases/latest/download/latest.json` and offers the new version. A version with a pre-release
  part (`2.1.0-beta.1`) is published as a GitHub pre-release, which `latest` — and so the updater —
  skips: that is the way to publish a build for testing.

1.x copies cannot install 2.x by themselves (docs/UPGRADING.md); the first line of the release
notes tells them where to download it.

**Android** has no version of its own: `android/app/build.gradle.kts` reads the workspace version
(`versionCode` = major·10000 + minor·100 + patch). The repository secrets `ANDROID_KEYSTORE_BASE64`,
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD` sign the APK with a
stable key; without them it carries the runner's debug key.

**Python 1.x** is no longer built: `installer.yml` was retired at the cutover and the last Python
state is the tag `python-final`.

## Step 1 — Does this change warrant a release?

**Release it** when a user running the installed app would notice a difference: importer or parser
rules, scheduler behaviour, the window or tray, Lecture Assist, the installer or autostart, a bug fix
in any of those.

**Don't release** doc-only edits, test-only changes, workflow/CI edits, or refactors with no
behavioural change. Those leave the number alone.

If the working tree already holds an **unpushed** bump, that number covers this change too. Don't
bump twice for one push.

## Step 2 — Read the current state

```bash
cd rust
cargo metadata --no-deps --format-version 1 | grep -o '"version":"[^"]*"' | head -1
gh release list --repo reyjendiz/AutoPara --limit 5
git grep -n "<current version>" -- . ':!*.lock'
```

The last line finds the docs that name the current version; rewrite the ones that describe the
artifact being shipped now and leave records of past releases alone.

## Step 3 — Propose the number, then wait

Decide which part you are moving and why, and agree it with the owner. For this project — a
single-user desktop app — semver reads as:

- **patch** (`2.0.0` → `2.0.1`): a fix. Behaviour the user already expected now works.
- **minor** (`2.0.0` → `2.1.0`): new or changed behaviour the user will notice. Most releases here.
- **major**: something that invalidates the user's data or forces a reinstall by hand. 2.0.0 is
  one: a new stack, and 1.x cannot update to it by itself.

## Step 4 — Apply it

Edit `version` under `[workspace.package]` in `rust/Cargo.toml`, then run any cargo command (for
example `cargo check`) so `Cargo.lock` follows. Nothing else holds a version: `tauri.conf.json` has
none and takes the crate's.

## Step 5 — Verify

```bash
cd rust
cargo xtask check
cd crates/autopara-app
cargo tauri build      # with TAURI_SIGNING_PRIVATE_KEY set; see RELEASE_CHECKLIST.md
```

The installer's file name and its version info must show the new number.

## Step 6 — Commit

Fold the bump into the commit carrying the change wherever that is still possible; otherwise make
it its own commit that says it cuts the release. After pushing, confirm rather than assume:

```bash
gh run list --workflow rust-release.yml --limit 3
```

If a release did not appear, the near-certain cause is that `v<version>` was already released.
