# AutoPara privacy policy

AutoPara is a desktop app that opens your classes and calls on time. It has no server and no account of its own.

## Google Calendar

If you choose "Sign in with Google", AutoPara asks for read-only access to the events of your primary calendar (`calendar.events.readonly`). It uses that access for one thing: about once a minute it reads your upcoming events (title, start and end time, meeting link) and shows them as calls in the app.

- The events are stored only in AutoPara's local database on your computer.
- The sign-in key (refresh token) is stored in your system's credential store: Windows Credential Manager or the macOS Keychain.
- Nothing from your calendar is sent anywhere else, sold, shared or used for ads. AutoPara talks only to Google's servers.
- AutoPara never creates, changes or deletes anything in your calendar.

"Sign out" in the app's settings deletes the key and the meetings it brought in. You can also remove AutoPara's access at any time at https://myaccount.google.com/permissions.

AutoPara's use of information received from Google APIs adheres to the [Google API Services User Data Policy](https://developers.google.com/terms/api-services-user-data-policy), including the Limited Use requirements.

## Everything else

Your timetable, settings and history stay on your computer. The app checks GitHub for new versions; that request contains no personal data.

## Contact

Questions: open an issue at https://github.com/reyjendiz/AutoPara/issues
