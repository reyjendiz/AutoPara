// The week and the day (Python `ui/week_grid.py`, `ui/class_card.py`): hourly rows from 08:00 to
// 18:00, one column per date, every class drawn across the time it really occupies.
import type { App, ClassRef } from "../app";
import type { Lesson } from "../bindings/Lesson";
import { h, style } from "../dom";
import { t } from "../i18n";
import { type IsoDate, dayName, dayShort, longDate, parseIso, toMinutes, weekday, groupsWord } from "../lib/calendar";
import { subjectColor, tint } from "../lib/color";
import {
  COLUMN_MIN_WIDTH, GUTTER_WIDTH, HEADER_HEIGHT, HOURS, HOUR_HEIGHT, cardBox, chooseLayout, nowOffset,
} from "../lib/grid";
import { occursOn, teacherName } from "../lib/lessons";
import { draggable } from "./drag";

/** Line heights the card's layout is chosen with: 9.5pt bold subject, 8pt teacher and time. */
const LINES = { chips: 18, subject: 16, teacher: 14, time: 14 };
const NOW_REFRESH_MS = 30_000;

function css(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** The name a class carries through a morph: within one view its place, across views its date. */
export function morphName(app: App, lesson: Lesson, date: IsoDate, placeIndex: number): string {
  return app.morph === "date" ? `c-${lesson.id}-${date}` : `c-${lesson.id}-p${placeIndex}`;
}

function stateOf(app: App, lesson: Lesson, date: IsoDate, nextKey: string | null): string {
  const status = app.statusOf(lesson, date);
  if (status === "opened" || status === "manual") return "opened";
  if (status === "skipped") return "skipped";
  if (status === "missed") return "missed";
  if (lesson.needs_link) return "nolink";
  return nextKey === `${lesson.id}|${date}` ? "next" : "normal";
}

const STATE_CHIP: Record<string, string> = {
  opened: "card_opened",
  missed: "card_missed",
  skipped: "card_skipped",
  nolink: "card_nolink",
};

/** One class on one date: chips, subject, teacher, detail, time — as many lines as its height
 * allows, in whole lines, the time going first (PORT_SPEC §E.5). */
export function classCard(app: App, lesson: Lesson, date: IsoDate, height: number, nextKey: string | null, wide: boolean): HTMLElement {
  const state = stateOf(app, lesson, date, nextKey);
  const color = state === "missed" ? css("--danger") : subjectColor(lesson.subject);
  const chips: HTMLElement[] = [];
  const stateKey = STATE_CHIP[state];
  if (stateKey) chips.push(h("span", { class: `chip ${state}` }, t(stateKey as never)));
  if (lesson.url) {
    const label = lesson.provider === "zoom" ? "Zoom" : lesson.provider === "google_meet" ? "Meet" : t("card_link");
    chips.push(h("span", { class: `chip ${lesson.provider}` }, label));
  }
  const groups = lesson.group_names.length;
  if (groups > 1) {
    chips.push(
      h("span", { class: "chip groups", "data-tip": t("card_shared", { groups: lesson.group_names.join(", ") }) },
        t("card_groups_chip", { count: String(groups), groups: groupsWord(groups) })),
    );
  }

  const teacher = teacherName(lesson);
  // A class without a teacher spends no height on one (Python `ClassCard._build`).
  const layout = chooseLayout(height, { ...LINES, teacher: teacher ? LINES.teacher : 0 });
  const detail = groups > 1 ? t("card_groups", { groups: lesson.group_names.join(", ") }) : lesson.url ? new URL(lesson.url).host : "";
  const subject = style(h("div", { class: "subject" }, lesson.subject), { "-webkit-line-clamp": String(layout.subjectLines) });
  const tip = [
    lesson.subject,
    teacher,
    `${lesson.start}–${lesson.end}`,
    lesson.group_names.length ? t("card_groups", { groups: lesson.group_names.join(", ") }) : "",
    lesson.url ?? t("card_no_link"),
    t("card_hint"),
  ].filter(Boolean).join("\n");

  const card = h(
    "div",
    {
      class: `card ${state}`,
      role: "button",
      tabindex: "0",
      "data-tip": tip,
      "aria-label": `${lesson.subject}, ${lesson.start}–${lesson.end}${teacher ? `, ${teacher}` : ""}`,
    },
    style(h("span", { class: "bar", "aria-hidden": "true" }), { background: color }),
    h("div", { class: "chips" }, ...chips),
    subject,
    layout.teacher && teacher ? h("div", { class: "teacher" }, teacher) : null,
    layout.detail && wide && detail ? h("div", { class: "teacher" }, detail) : null,
    h("div", { class: "stretch" }),
    layout.time ? h("div", { class: "time" }, `${lesson.start}–${lesson.end}`) : null,
  );
  if (state === "normal" || state === "next") {
    const amount = Number(css("--subject-tint")) || 0.16;
    card.style.setProperty("--card-fill", tint(css("--card-bg"), subjectColor(lesson.subject), amount));
  }
  return card;
}

/** Hide the chips that do not fit, most important first; the first always stays. */
function fitChips(card: HTMLElement): void {
  const row = card.querySelector<HTMLElement>(".chips");
  if (!row) return;
  const room = row.clientWidth;
  let used = 0;
  [...row.children].forEach((chip, index) => {
    const element = chip as HTMLElement;
    element.hidden = false;
    const need = element.offsetWidth + (used ? 5 : 0);
    const fits = index === 0 || used + need <= room;
    element.hidden = !fits;
    if (fits) used += need;
  });
}

export function renderGrid(app: App, days: IsoDate[], lessons: Lesson[]): HTMLElement {
  const today = app.today();
  const single = days.length === 1;
  const nextKey = app.nextKey(lessons);
  const grid = h("div", { class: "grid", role: "grid", "aria-rowcount": String(HOURS.length + 1) });
  style(grid, {
    "grid-template-columns": `${GUTTER_WIDTH}px repeat(${days.length}, minmax(${COLUMN_MIN_WIDTH}px, 1fr))`,
    "grid-template-rows": `${HEADER_HEIGHT}px repeat(${HOURS.length}, ${HOUR_HEIGHT}px) 1fr`,
  });

  const header = (date: IsoDate | null, short: boolean) => {
    if (!date) return h("div", { class: "day-head", role: "columnheader" });
    const day = parseIso(date);
    return h(
      "div",
      { class: date === today ? "day-head today" : "day-head", role: "columnheader" },
      h("span", { class: "name" }, short ? dayShort(weekday(date)) : dayName(weekday(date))),
      h("span", { class: "date-circle", "data-tip": t("grid_date_tip", { date: String(day.getDate()), month: longDate(date).split(" ")[1] ?? "", year: String(day.getFullYear()) }) }, String(day.getDate())),
    );
  };
  grid.append(style(header(single ? (days[0] ?? null) : null, true), { "grid-row": "1", "grid-column": "1" }));
  days.forEach((date, column) => grid.append(style(header(single ? null : date, false), { "grid-row": "1", "grid-column": String(column + 2) })));

  HOURS.forEach((hour, row) => {
    const label = `${String(hour).padStart(2, "0")}:00`;
    grid.append(style(h("div", { class: "gutter-cell", role: "rowheader" }, label), { "grid-row": String(row + 2), "grid-column": "1" }));
    days.forEach((date, column) => {
      const [dd, month] = longDate(date).split(" ");
      const cell = h("div", {
        class: date === today ? "cell today" : "cell",
        role: "gridcell",
        tabindex: "-1",
        "data-drop-date": date,
        "data-drop-start": label,
        "data-tip": `${t("grid_slot_tip", { day: dayName(weekday(date)), date: dd ?? "", month: month ?? "", time: label })}\n${t("grid_slot_hint")}`,
        "aria-label": t("grid_slot_tip", { day: dayName(weekday(date)), date: dd ?? "", month: month ?? "", time: label }),
        onclick: (event) => void app.slotClicked(date, hour, (event as MouseEvent).clientX, (event as MouseEvent).clientY),
      });
      grid.append(style(cell, { "grid-row": String(row + 2), "grid-column": String(column + 2) }));
    });
  });

  const cards: HTMLElement[] = [];
  days.forEach((date, column) => {
    const layer = style(h("div", { class: "column-cards" }), { "grid-row": `2 / span ${HOURS.length}`, "grid-column": String(column + 2) });
    lessons.filter((lesson) => occursOn(lesson, date)).forEach((lesson) => {
      const { top, height } = cardBox(toMinutes(lesson.start), toMinutes(lesson.end));
      const card = classCard(app, lesson, date, height, nextKey, single);
      style(card, { top: `${top}px`, height: `${height}px`, "view-transition-name": morphName(app, lesson, date, single ? 0 : column) });
      const ref: ClassRef = { lesson, date };
      draggable(card, ref, (x, y) => void app.joinClass(ref, x, y), (target) => void app.dropClass(ref, target.date, target.start ?? lesson.start));
      card.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        void app.classMenu(ref, event.clientX, event.clientY);
      });
      card.addEventListener("keydown", (event) => {
        const box = card.getBoundingClientRect();
        if (event.key === "Enter" || event.key === " ") void app.joinClass(ref, box.left, box.bottom);
        else if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) void app.classMenu(ref, box.left, box.bottom);
        else return;
        event.preventDefault();
      });
      layer.append(card);
      cards.push(card);
    });
    grid.append(layer);
  });

  // The "now" line across today's column, at half strength, with the time in the gutter.
  const line = h("div", { class: "now-line", "aria-hidden": "true" });
  const badge = h("div", { class: "now-badge", "aria-hidden": "true" });
  grid.append(line, badge);
  const placeNow = () => {
    const column = days.indexOf(app.today());
    const now = app.now();
    const offset = nowOffset(now.getHours() * 60 + now.getMinutes());
    const anchor = column >= 0 ? (grid.querySelector(`.cell[data-drop-date="${days[column]}"]`) as HTMLElement | null) : null;
    const show = Boolean(anchor) && offset !== null;
    line.hidden = badge.hidden = !show;
    if (!show || !anchor || offset === null) return;
    const y = HEADER_HEIGHT + offset;
    style(line, { top: `${y - 1}px`, left: `${anchor.offsetLeft}px`, width: `${anchor.offsetWidth}px` });
    badge.textContent = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
    style(badge, { top: `${y}px`, right: `calc(100% - ${GUTTER_WIDTH - 4}px)` });
  };

  const scroll = h("div", { class: "calendar" }, grid);
  const settle = () => {
    placeNow();
    cards.forEach(fitChips);
  };
  new ResizeObserver(settle).observe(grid);
  const timer = window.setInterval(() => (scroll.isConnected ? placeNow() : window.clearInterval(timer)), NOW_REFRESH_MS);
  return scroll;
}
