#!/usr/bin/env bash
# Runs inside the emulator job (see .github/workflows/android-emulator.yml), with an emulator booted.
#
#  1. Installs the APK that was published as the release, launches it and checks it neither crashes
#     nor exits -- the same file a user downloads.
#  2. Uninstalls it (the debug build is signed with a different key) and runs the instrumented tests.
set -euo pipefail

out="${OUT_DIR:-$PWD/emulator-out}"
mkdir -p "$out"

version=$(sed -n 's/.*versionName = "\([^"]*\)".*/\1/p' android/app/build.gradle.kts | head -n1)
apk="$out/AutoPara-$version.apk"
echo "== Published release APK, version $version"
curl -fsSL -o "$apk" "https://github.com/${GITHUB_REPOSITORY}/releases/download/android-v${version}/AutoPara-${version}.apk"
adb install -r "$apk"
adb logcat -c
adb shell monkey -p app.autopara -c android.intent.category.LAUNCHER 1
sleep 10
adb exec-out screencap -p > "$out/release-launch-${PROFILE:-phone}.png"
if ! adb shell pidof app.autopara > /dev/null; then
  echo "The app is not running after launch"; adb logcat -d | tail -80; exit 1
fi
if adb logcat -d | grep -E "FATAL EXCEPTION"; then
  echo "The app crashed on launch"; adb logcat -d | tail -120; exit 1
fi
echo "The release APK launched and stayed up."
adb uninstall app.autopara > /dev/null

echo "== Instrumented tests"
set +e
(cd android && ./gradlew :app:connectedDebugAndroidTest --console=plain)
status=$?
set -e

adb pull /sdcard/Android/data/app.autopara/files/screenshots "$out/screenshots-${PROFILE:-phone}" > /dev/null 2>&1 || true
adb logcat -d > "$out/logcat-${PROFILE:-phone}.txt" || true
exit $status
