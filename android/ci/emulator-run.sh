#!/usr/bin/env bash
# Runs inside the emulator job (see .github/workflows/android-emulator.yml), with an emulator booted.
#
#  1. Installs the release APK (the one published with the release, else one built here), launches it and checks it neither crashes
#     nor exits -- the same file a user downloads.
#  2. Uninstalls it (the debug build is signed with a different key) and runs the instrumented tests.
set -euo pipefail

out="${OUT_DIR:-$PWD/emulator-out}"
mkdir -p "$out"

version=$(sed -n 's/^__version__ = "\([^"]*\)".*/\1/p' autopara/__init__.py | head -n1)
apk="$out/AutoPara-$version.apk"
echo "== Release APK, version $version"
if curl -fsSL -o "$apk" "https://github.com/${GITHUB_REPOSITORY}/releases/download/v${version}/AutoPara-${version}.apk"; then
  echo "Using the APK published with release v$version."
else
  # A push that bumps the version runs this before the release has its APK: test the build instead.
  echo "v$version has no APK yet; building the release APK from this checkout."
  (cd android && ./gradlew :app:assembleRelease --console=plain)
  cp android/app/build/outputs/apk/release/*.apk "$apk"
fi
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
