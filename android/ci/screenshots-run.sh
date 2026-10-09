#!/usr/bin/env bash
# Runs inside the emulator job of android-screenshots.yml: plays ScreenshotsTest and collects its pictures.
set -euo pipefail

out="${OUT_DIR:-$PWD/emulator-out}/screenshots"
mkdir -p "$out"

(cd android && ./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=app.autopara.ScreenshotsTest --console=plain)

# The test saved them under /data/local/tmp (written by the shell, so they outlive the app's uninstall).
adb pull /data/local/tmp/shots/. "$out/"
ls -la "$out"
test -n "$(ls "$out"/*.png)"
