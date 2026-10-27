# Visual parity with the Python app (plan step 5.11)

`cargo xtask screenshots` renders the six reference scenes into `target/screenshots/` from the demo
backend: the sample timetable, the clock pinned to Monday 2026-03-02 09:40:20, 1240×800, Inter
(as the reference pictures are). Compare each with the file of the same name in `docs/`.

| Scene | Status | Remaining differences |
|---|---|---|
| `week-grid` | close | Same layout, cards, chips and now line; small vertical offsets in card text and hour labels are visible side by side (not measured). |
| `week-grid-dark` | close | As the light week. |
| `month-view` | close | Same tiles and chips; chip spacing reads slightly tighter (not measured). |
| `day-view` | close | As the week. |
| `edit-dialog` | layout matches | The Python picture is the dialog alone (440×444); ours is the window with the dialog over it. The date and time fields are the webview's own controls (calendar and clock pickers), not Qt's spin boxes. |
| `settings` | layout matches | Same framing difference. The settings list is taller than the window, so the dialog scrolls (the Python picture is 915 px tall). The new "Асистент пари" section and the diagnostics button sit at the bottom. |

Differences by design, not to be "fixed":

- **Morph between periods.** The new calendar cross-fades and each class grows from its old box to
  its new one (View Transitions), but the empty calendar does not drift 14 px in the direction of
  time as the Python overlay did.
- **Text on macOS.** The Python app scaled its point sizes by 1.15 on a Mac; the webview uses CSS
  points everywhere, so Mac text is a little larger than in 1.x and the same size as on Windows.
- **Windows font.** Without `?font=inter` the app uses Segoe UI on Windows, as the Python app did;
  the reference pictures (and the screenshot command) use Inter.
- **Scroll bars** follow the webview's style, not the thin Qt ones.

How the comparison was made: side by side at full size in the browser pane, checking positions of
the title, pill, segmented control, rail, header, every card, the now line and the chips. Pixel
diffing is not used: text rasterisation differs between Qt and the webview everywhere.
