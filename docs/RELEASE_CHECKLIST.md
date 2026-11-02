# Release checklist (Rust app)

A manual smoke test on a clean virtual machine, run before a release goes to `main`. It covers what
CI cannot: the installer, the tray, autostart and a real update. Use a fresh Windows 10 or 11 VM
(no AutoPara, no Python) and, for the Mac images, a Mac of each kind. Tick each line; anything that
fails blocks the release.

## Once per repository

- [ ] The updater's private key is in the repository secret `TAURI_SIGNING_PRIVATE_KEY` (the
      contents of `%USERPROFILE%\.tauri\autopara-updater.key`, not its path), and its password, if
      it has one, in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- [ ] The key file is backed up somewhere other than this PC. Without it no installed copy can
      ever be updated again: a new key means every user reinstalls by hand.

## Before pushing

- [ ] `cargo xtask check` is green in `rust/`.
- [ ] The workspace version in `rust/Cargo.toml` is new (no `v<version>` release yet) and agreed.
      A version with a `-` part (`2.0.0-beta.1`) is published as a pre-release, which installed
      copies do not update to.
- [ ] `cargo tauri build` (PowerShell, in `rust/crates/autopara-app`) produces
      `rust/target/release/bundle/nsis/AutoPara_<version>_x64-setup.exe`; test with that file.

## Windows

1. **Install**
   - [ ] Double-click the Setup.exe: no UAC prompt; the pages are welcome, components, folder,
         progress, finish, all in Ukrainian.
   - [ ] Components show `AutoPara (обов'язково)` (greyed), `Ярлик на робочому столі` and
         `Запускати разом із Windows`, both ticked; the hint text appears on hover.
   - [ ] The folder defaults to `%LOCALAPPDATA%\Programs\AutoPara`.
   - [ ] Afterwards: Start Menu folder `AutoPara` with `AutoPara` and `Видалити AutoPara`; the
         desktop shortcut; `AutoPara — автозапуск пар` in Settings → Apps with the version;
         `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` value `AutoPara` =
         `"C:\Users\<you>\AppData\Local\Programs\AutoPara\AutoPara.exe" --hidden`.
   - [ ] On a VM without WebView2 (Windows 10 LTSC, or after removing it), the installer installs it
         and the app starts.
2. **First import**
   - [ ] Finish with `Запустити AutoPara` ticked: the window opens empty and asks for a timetable.
   - [ ] Drop the sample timetable (`tests/sample_timetable.py` output, never the real one): course
         and group can be chosen and the week grid fills.
3. **Join**
   - [ ] Add a class starting in three minutes with an `https://` link: a reminder appears and the
         link opens in the default browser one minute before the start, once.
4. **Close to tray**
   - [ ] Closing the window leaves the tray icon; its menu shows the window again; `Вийти` quits.
5. **Autostart**
   - [ ] Sign out and in: AutoPara starts hidden in the tray.
   - [ ] Turning autostart off in the app's settings removes the `Run` value; on again restores it.
6. **Update** (needs two versions; with a test release on a fork or a pre-release turned into a
   release)
   - [ ] With the older version installed and running, the update strip appears within a minute
         of start (or after `Перевірити оновлення`).
   - [ ] Download, then install: the app closes, the installer runs without a window, and the new
         version starts by itself with the timetable intact.
   - [ ] After turning autostart off in the old version, the update did not turn it back on; the
         shortcuts are where they were.
7. **Uninstall**
   - [ ] From Settings → Apps: confirm, then `Видалити також збережений розклад і налаштування?`
         with the `%APPDATA%\AutoPara` path. **No**: the program folder, shortcuts, `Run` value and
         Add/Remove entry are gone; `%APPDATA%\AutoPara` is still there.
   - [ ] Reinstall: the timetable is back without a new import.
   - [ ] Uninstall again and answer **Yes**: `%APPDATA%\AutoPara`, `%APPDATA%\com.autopara.app`
         and `%LOCALAPPDATA%\com.autopara.app` are gone too.
   - [ ] Silent: `AutoPara_<version>_x64-setup.exe /S` installs with both shortcuts and autostart
         and no window; `"%LOCALAPPDATA%\Programs\AutoPara\uninstall.exe" /S` removes it and keeps
         `%APPDATA%\AutoPara`.
8. **Upgrade from 1.x** (a VM with AutoPara 1.10.0 installed, a timetable imported, a class
   marked skipped, and the theme and lead time changed; see docs/UPGRADING.md)
   - [ ] With 2.0 released, 1.10.0's update strip names the new version and its detail line is the
         release notes' first line (where to download); it offers no download.
   - [ ] Run the 2.0 Setup.exe while 1.x is running: it asks to close AutoPara, then offers to
         remove the older version.
   - [ ] Afterwards there is one AutoPara in Settings → Apps (2.0.0), one Start Menu folder, no
         `_internal` folder, and the `Run` value `AutoPara` is unchanged.
   - [ ] The new app shows the same course and group, the same classes, the skipped mark, the
         theme and the lead time, without a new import. Its log says `imported the previous
         version's data`.
   - [ ] `%APPDATA%\AutoPara\autopara.db` and `schedules\` are unchanged (compare hashes taken
         before); `autopara2.db` and `timetables\` are new.
   - [ ] Sign out and in: only 2.0 starts, and a class opens once.

## macOS (each of Apple Silicon and Intel)

- [ ] `AutoPara-<version>.dmg` on Apple Silicon and `AutoPara-Intel-<version>.dmg` on Intel open
      and show AutoPara to drag into Applications.
- [ ] The first start goes through the context menu → `Відкрити` (the app is not signed); after
      that it starts normally.
- [ ] First import, join, close to tray (menu bar icon) and autostart (`~/Library/LaunchAgents/
      com.autopara.app.plist`) as on Windows.
- [ ] Update: the strip offers the new version; install replaces the app and restarts it.

## After pushing

- [ ] The `Rust release` workflow is green and the release `v<version>` has
      `AutoPara-<version>-Setup.exe`, `AutoPara-<version>.dmg`, `AutoPara-Intel-<version>.dmg`, the
      two `.app.tar.gz` archives, a `.sig` for the Setup.exe and each archive, and `latest.json`.
- [ ] `https://github.com/reyjendiz/AutoPara/releases/latest/download/latest.json` serves that
      version (pre-releases excepted).
