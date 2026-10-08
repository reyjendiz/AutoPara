# AutoPara for Android

The Android port of AutoPara: import the university's `.docx` timetable, browse it on a phone or a
tablet, and get a notification exactly one minute before each class with a one-tap **Join** that
opens the meeting in the native Zoom or Google Meet app.

Kotlin, Jetpack Compose, Material 3, MVVM. Min SDK 26 (Android 8.0), target SDK 35.

## Layout

```
android/
├── core/   pure Kotlin/JVM -- no Android classes, unit-tested on any machine
│   ├── importer/   DocxReader, ScheduleParser, Normalize   (port of autopara/importer)
│   ├── schedule/   LessonFilter, NextUp, Reminders, CalendarMath
│   ├── link/       MeetingLink  (which URLs are openable, which app handles them)
│   └── model/      Lesson, Course, Group, Occurrence, SubjectColor
└── app/    the Android application
    ├── data/       Room database, DataStore settings, ScheduleRepository
    ├── notify/     ReminderScheduler (AlarmManager), ReminderReceiver, RescheduleReceiver,
    │               JoinActivity, Notifications
    ├── link/       MeetingLauncher  (native app -> browser -> store)
    └── ui/         MainViewModel + Compose screens (day / week / month, filter, detail, settings)
```

The split is deliberate. Everything that decides *what the timetable says* and *when to remind*
lives in `core`, so it is verified without an emulator. The parser is checked field-for-field
against the output of the desktop (Python) parser on the same synthetic timetable
(`core/src/test/resources/sample_timetable.golden.json`), so the two implementations cannot drift.

## Architecture

* **MVVM, unidirectional data flow.** `MainViewModel` combines the Room/DataStore flows, the clock
  and the user's selection into one immutable `UiState`; the Compose screens render it and send
  events back. No state lives in a composable that must survive rotation.
* **Manual DI.** `AutoParaApp` owns an `AppContainer` (database, settings, repository, reminder
  scheduler, an application-wide coroutine scope). The receivers need the same objects with no UI
  around, and a DI framework would only add build risk to an app this size.
* **Fire-once.** `occurrences` has a `(lessonId, day)` primary key; a reminder is claimed with an
  `INSERT … IGNORE`, so a late, duplicated or re-armed alarm cannot notify twice, and a date the user
  skipped is never notified.

## Adaptive layout

`calculateWindowSizeClass` drives the shape on every configuration change (rotation, split-screen,
foldables):

| Window            | Navigation        | Calendar                         | Class details          |
|-------------------|-------------------|----------------------------------|------------------------|
| Compact (phone)   | bottom bar        | day list + week strip, month     | bottom sheet           |
| Medium            | navigation rail   | day, **week grid**, month        | bottom sheet           |
| Expanded (tablet) | navigation rail   | day, week grid, month            | permanent side pane    |

A phone held sideways is wide but short, so it gets the rail too.

## Meeting links (no WebView)

`MeetingLauncher.open` hands the URL to the native app and degrades in three steps:

1. `ACTION_VIEW` addressed to the provider's package (`us.zoom.videomeetings`, or the Meet app) —
   both register verified HTTPS App Links, so the meeting opens inside the app;
2. if that app is not installed, any browser — the user still gets to the class, and a snackbar
   offers **Get app**;
3. if there is no browser either, the app's Play Store page.

Only `http(s)` links are ever opened (the text comes from a document). The `<queries>` block in the
manifest provides the package visibility Android 11+ requires.

## Reminders — exactly one minute before

* `AlarmManager.setExactAndAllowWhileIdle(RTC_WAKEUP, …)`, one alarm per upcoming class, armed for
  the next 7 days (`Reminders.upcoming`). WorkManager is deliberately not used: it is deferrable and
  can run minutes late.
* The alarm targets `ReminderReceiver`, registered in the manifest, so it fires with the app closed.
* The window is re-armed whenever the timetable, group, skips or the reminders switch change, every
  time an alarm fires, and after anything that drops alarms — reboot, app update, clock or time-zone
  change, exact-alarm permission change (`RescheduleReceiver`).
* Permissions: `POST_NOTIFICATIONS` (asked at runtime on Android 13+), `SCHEDULE_EXACT_ALARM`
  (Android 12+: the user must allow *Alarms & reminders*; a banner links to the right screen and
  disappears once granted), `RECEIVE_BOOT_COMPLETED`. Without the exact-alarm permission the app
  falls back to `setAndAllowWhileIdle`, which can be a few minutes late.
* The notification's **Join** button is a `PendingIntent.getActivity` to `JoinActivity`, which opens
  the meeting and finishes. It is an activity because Android 12+ blocks notification trampolines
  through receivers and services. **Skip** marks that date skipped.

### Differences from the desktop app

* The desktop app opens the link by itself at the start of the lead window. Android forbids an app
  from launching an activity out of the background, so the reminder carries a one-tap **Join**
  instead.
* Editing, adding or moving classes, drag and drop, and the updater are not ported yet; the Android
  app imports and reads a timetable.
* On Google Play, `SCHEDULE_EXACT_ALARM` on Android 13+ is limited to alarm/calendar-type apps. If
  the app is published there, either declare it as such or switch to `USE_EXACT_ALARM`.

## Building

```sh
cd android
./gradlew -PcoreOnly=true :core:test      # parser + scheduling rules; needs only a JDK
./gradlew :app:assembleDebug              # needs the Android SDK (API 35)
./gradlew :app:installDebug               # onto a connected device or emulator
```

GitHub Actions (`.github/workflows/android.yml`) runs both on every change under `android/` and
attaches the debug APK to the run.

To try the reminders quickly on a device, import a timetable, then set the system clock a couple of
minutes before a class (or `adb shell cmd alarm` / the date-time settings) and watch for the
notification.
