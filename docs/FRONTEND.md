# FRONTEND — screens, components, state, styling

> Read `ARCHITECTURE.md` and `BACKEND.md` first. Update this file whenever a screen, component, or
> styling convention changes. The full inventory of 1.x screens, strings and tokens the interface
> reproduces is `rust/PORT_SPEC.md` §E–§F; how close it came is `rust/PARITY_UI.md`.

The interface is TypeScript in `rust/crates/autopara-app/ui/src`, built by Vite and shown in the
system webview. There is no framework: `dom.ts` has one helper, `h(tag, attributes, ...children)`,
and each view is a function that renders from data.

## Language

**Every string the user can see is Ukrainian**, and every one is a key in
`rust/crates/autopara-core/i18n/uk.json`, read through `i18n.ts` (`t(key)`, `tf(key, vars)`; the
keys are type-checked against the file). Rust reads the same file, so the tray, notifications and
installer share it. Ukrainian plural forms and dates are in `lib/calendar.ts`.

Subject and teacher text is the exception, and only because it is data: it is reproduced exactly as
the document wrote it.

## Screen map

| Screen | Module | Role |
|---|---|---|
| Main window | `app.ts` | State, reload and actions; owns the date being looked at. |
| Backend | `backend.ts`, `ipc.ts` | Everything the interface asks of the outside world. The real app calls the Rust commands; `?demo` answers from `demo/sample.json` with a pinned clock. |
| Rail | `ui/rail.ts` | The app mark, the floating black pill of icon buttons, a quiet clock. |
| Navigation strip | `ui/nav.ts` | The period, the "now / next" pill, the День / Тиждень / Місяць control, "Сьогодні" and the arrows. |
| Search and filters | `ui/filters.ts` | Search box, subject filter, group filter, the count. Owns no data. |
| Week and day | `ui/grid.ts` (geometry in `lib/grid.ts`) | Hourly rows 08:00–18:00, one column per date, cards placed by their real times. |
| Month | `ui/month.ts` | Six weeks of day tiles with up to three chips each. |
| Moving a class | `ui/drag.ts` | Pointer-event dragging in the week, the day and the month. |
| Missed-class banner | `ui/banner.ts` | "Пара вже почалася": connect or close; queues. |
| Update strip | `ui/updates.ts` | Offered → downloading → ready to install. |
| First run | `ui/landing.ts` | The import landing and the drop well it shares with the import dialog. |
| Dialogs | `ui/dialogs/` | `import.ts`, `group.ts` (another course and group), `edit.ts` (a class), `settings.ts`, `assist.ts` (Lecture Assist consent and settings). |
| Popups | `ui/popups.ts` | Rounded tooltips, context menus and message boxes drawn by the page. |
| Glyphs | `icons.ts` | Every icon, drawn on a 24-unit grid in `currentColor`. |
| Assistant panel | `assist/` | Lecture Assist beside the class in the class tab (docs/ASSIST.md). |

Pure logic lives in `lib/` (dates, colours, grid geometry, what a lesson does on a date, search and
the "now / next" pill), so the week, the day, the month and the counts can never disagree, and it
is what `vitest` tests.

## Rail

A **72 px rail down the left**, not a toolbar across the top: the week grid is wide and short, and a
top bar spends the height a calendar is always short of. Top to bottom: the app mark, then the
**black pill** holding the circular add button and the icon buttons (import, another course and
group, settings, theme). **Nothing on it is written**; each button's meaning is its Ukrainian
tooltip, and the course and group are the mark's tooltip.

The import glyph's arrow points **up**: the button hands a file the user already has to AutoPara;
pointing it down read as "download".

## There is no status bar and no edit mode

The grid already shows today tinted, the next class outlined and every class's state on its own
card. And the grid is editable from the moment it opens: joining is the left button, everything
else is behind the right-click menu, so nothing destructive happens without a second, named choice.

## Views: day, week, month

The choice is the `view_mode` setting, so it survives a restart; the date being looked at does not —
the app opens on today. The arrows step a day, a week or a month (a month step from the 31st lands
on the last day of a shorter month), and "Сьогодні" returns to today.

- **День** is the week grid with one column across the whole work area. A class gets every chip,
  and a tall one also a detail line: the groups that share it, else where its link leads.
- **Тиждень**: Mon–Sat, plus Sunday when a lesson lands there.
- **Місяць** is always six weeks, so it never changes height. A tile shows the date and up to three
  chips in time order; the rest are counted as "ще N". A verdict is spelled in the chip (✓ opened,
  ✕ skipped, ! not opened), not only coloured. Clicking a tile opens that day; the plus under the
  pointer creates a class on that date.

## Week grid

- **A class is placed by its real time, not by a slot.** One minute is a fixed number of pixels
  (`lib/grid.ts`), so 09:30–10:50 covers the bottom half of the 09:00 row and most of the 10:00 one.
  A class outside 08:00–18:00 is clamped to the edge, and the editor accepts any time.
- A lesson spanning several pairs (`BACKEND.md` R5, R12) is one card. A double class is one card
  for the same reason it is one browser tab: it never stopped.
- Today's column is tinted and crossed by a "now" line with the time, refreshed every 30 s.
- **Empty slots**: clicking an empty hour offers `Створити пару` with that date and hour filled in.
- **Dragging** a card moves it to the dropped date and hour, keeping its length. Moving a class that
  repeats asks whether to move **only that date** (a `lesson_skips` row plus a one-off) or the whole
  series. A press counts as a click only if the pointer never travelled far enough to start a drag.
  Dragging uses pointer events, not HTML drag and drop, which the webview's file drop disables on
  Windows.

## Class card

A chip row (state, provider, a group chip for a shared session), then the subject, the teacher and
the time. The subject's colour is a bar at the left edge and a tint of the card (`lib/color.ts`: the
same MD5 pick of seven pastels as 1.x, so a subject keeps its colour across the two apps).

**A card is exactly as tall as its class is long**, so it drops whole lines in a fixed order — the
time first, since the card's position already says it — and ends the last kept line with "…".
Chips give way to width in priority order: state, provider, group. The tooltip carries the whole
subject, the teacher, the exact times, every group and the link.

| State | Treatment |
|---|---|
| upcoming | normal card |
| next up (soonest today) | ink outline |
| opened | `✓ відкрито`, muted |
| missed | `не відкрито`, warning stripe |
| skipped | `пропущено`, dashed and muted |
| no link | dashed border + `без посилання` |

**Left click joins the class.** With no link, it raises the menu, whose first item is
`Додати посилання…`. **Right click** (or the Menu key) opens: Відкрити посилання · Редагувати… ·
Позначити як відкриту · Позначити як пропущену · (Зняти позначку) · Видалити пару. A click on a
date that has not come yet opens the link without claiming that date; the marks do apply to the date
on screen, which is how a future class is skipped in advance.

## Missed-class banner

Hidden unless a class is waiting for an answer. It names the class and offers **Підключитися
зараз** and **Закрити**; nothing opens until one is pressed, and the window comes forward when one
arrives. `Закрити` marks the class missed. Several queue, with `ще N у черзі`.

## Class editor

A class is *Один раз* (a date), *Щотижня до дати* (a series with an end) or *Щотижня, без кінця* (a
weekly row with a weekday). The form shows only the fields the kind needs, and a line says in words
what will happen ("Щотижня, у понеділок: з 12 жовтня по 9 листопада — 5 разів."). An imported class
shows no kind: it stays a row of the timetable. Date and time fields are the webview's own
(`<input type="date|time">`); the end follows the start until the user moves it. Only `https://`
links are accepted; a command error is a code (`invalid_url`, …) that the dialog turns into a
Ukrainian sentence.

## Settings

Lead time · class length · theme · notifications (and how many minutes ahead) · what to do about a
missed class · autostart · updates · diagnostics (copies a summary and the log's tail) · the way
into Lecture Assist. The catch-up section says plainly that a class already running when AutoPara
starts is never opened unasked, even in "open immediately" mode.

## Import

On first run the window *is* the import landing: a drop well and `Обрати файл…`. A `.docx` can be
dropped anywhere on the window — the webview's file drop brings its real path — and only `.docx` is
accepted. The import dialog previews courses and groups (`preview_timetable`) before anything is
written, and opens on the previously chosen course and group when the new document has them.
`Обрати інший курс і групу…` on the rail switches group without reading the file again.

## Update strip

A neutral card: "Доступна нова версія AutoPara {v}" with Що нового, Пропустити версію, Пізніше and
Завантажити; then progress; then "Встановити й перезапустити". Installing is always a click here.

## Motion

Moving between periods and views is a morph through the View Transitions API: a class keeps a
`view-transition-name` (its place within one view, its date across views), so it grows from its old
box into the new one, and everything else cross-fades. The segmented control's pill glides.
The same morph (`dom.ts` `morph`) runs wherever the screen changes on a user's action: the two top
tabs, the theme, the missed-class and update banners, and the assistant's panel folding to its
header. Dialogs only fade in by CSS: during a morph the schedule's named cards are drawn above
everything else, so a dialog growing from its button was covered by them.
`prefers-reduced-motion` turns all motion off. Motion never delays what the window does.

## Theme and styling

- Colours, radii, spacing, shadows, fonts and durations are CSS custom properties in
  `styles/tokens.css`, one block per theme; `styles/app.css` uses only the tokens. **Never write a
  literal colour in a rule**: it looks right in one theme and wrong in the other.
- `system` follows `prefers-color-scheme`; the rail toggle pins the opposite of what is showing.
- **The accent is ink, not a hue**: near-black in the light theme, near-white in the dark. Pastel
  colour appears only where it means something (a subject, a status).
- The font is Segoe UI on Windows, Inter elsewhere (bundled, OFL); `?font=inter` forces Inter, which
  the screenshots use. The same CSS sizes apply on macOS (1.x scaled its points by 1.15).

## State management

The database is the source of truth. The window reloads from the backend after every change and on
these events from Rust:

- `scheduler` — a class was opened, offered (banner) or recorded missed; reminders are system
  notifications sent from Rust.
- `update` — the update strip's state.
- `tray-action` — a tray item that needs the window (import, settings, check for updates).
- `assist`, `assist-download`, `assist-class-over` — Lecture Assist (docs/ASSIST.md).

The window's only state of its own is the date it is looking at and the open dialog, which can never
go stale.

## Tests

`npm test` (vitest) covers `lib/` (dates, Ukrainian forms, grid geometry, filters, the "now / next"
pill), `i18n`, the IPC wrappers and the assistant panel's text pieces. `npm run typecheck` and `npm run
lint` are part of `cargo xtask check`. The look is compared by eye against the 1.x pictures with
`cargo xtask screenshots` (`rust/PARITY_UI.md`).
