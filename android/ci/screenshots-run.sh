#!/usr/bin/env bash
# Runs inside the emulator job of android-screenshots.yml: plays ScreenshotsTest and collects its pictures.
set -euo pipefail

out="${OUT_DIR:-$PWD/emulator-out}/screenshots"
mkdir -p "$out"

(cd android && ./gradlew :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=app.autopara.ScreenshotsTest --console=plain)

# The pictures are in the app's own storage. Try the shared location first, then the debug-build route.
adb pull /sdcard/Android/data/app.autopara/files/screenshots/. "$out/" > /dev/null 2>&1 || true
if ! ls "$out"/*.png > /dev/null 2>&1; then
  adb exec-out run-as app.autopara tar c -C files screenshots | tar x -C "$out/.." 
fi
ls -la "$out"
test -n "$(ls "$out"/*.png)"
