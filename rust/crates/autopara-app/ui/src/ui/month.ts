// The month (Python `ui/month_view.py`): six weeks of day tiles, each with up to three chips; the
// rest are counted. A tile opens its day; the plus creates a class on it; a chip can be dragged to
// another day.
import type { App, ClassRef } from "../app";
import type { Lesson } from "../bindings/Lesson";
import { h, style } from "../dom";
import { t } from "../i18n";
import { icon } from "../icons";
import { dayName, dayShort, longDate, monthGrid, pairsWord, parseIso, toMinutes, weekday } from "../lib/calendar";
import { subjectColor, tint } from "../lib/color";
import { occursOn, teacherName } from "../lib/lessons";
import { draggable } from "./drag";
import { morphName } from "./grid";

const MAX_CHIPS = 3;
const MARKS: Record<string, string> = { opened: "✓", manual: "✓", skipped: "✕", missed: "!" };

function css(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export function renderMonth(app: App, lessons: Lesson[]): HTMLElement {
  const today = app.today();
  const month = parseIso(app.anchor).getMonth();
  const days = monthGrid(app.anchor);
  const body = h("div", { class: "month-body", role: "grid" });
  const tiles: HTMLElement[] = [];

  days.forEach((date, index) => {
    const entries = lessons.filter((l) => occursOn(l, date)).sort((a, b) => toMinutes(a.start) - toMinutes(b.start));
    const day = parseIso(date);
    const classes = ["tile"];
    if (day.getMonth() !== month) classes.push("other");
    if (date === today) classes.push("today");
    const [dd, mm] = longDate(date).split(" ");
    const tile = h("div", {
      class: classes.join(" "),
      role: "gridcell",
      tabindex: "0",
      "data-drop-date": date,
      "data-tip": entries.length ? t("month_count", { count: String(entries.length), pairs: pairsWord(entries.length) }) : "",
      "aria-label": t("month_day_tip", { day: dayName(weekday(date)), date: dd ?? "", month: mm ?? "" }),
      onclick: () => app.openDay(date),
      onkeydown: (event) => {
        if ((event as KeyboardEvent).key === "Enter") app.openDay(date);
      },
    });
    const plus = h(
      "button",
      {
        class: "plus",
        "aria-label": t("month_add"),
        "data-tip": t("month_add"),
        onclick: (event) => {
          event.stopPropagation();
          void app.editClass(null, date);
        },
      },
      icon("add", 14),
    );
    tile.append(
      h("div", { class: "top" },
        h("span", { class: "date-circle", "data-tip": t("month_day_tip", { day: dayName(weekday(date)), date: dd ?? "", month: mm ?? "" }) }, String(day.getDate())),
        plus),
    );
    entries.slice(0, MAX_CHIPS).forEach((lesson) => tile.append(chip(app, lesson, date, index)));
    const more = h("div", { class: "month-more", hidden: true });
    tile.append(more);
    tile.dataset.total = String(entries.length);
    tiles.push(tile);
    body.append(tile);
  });

  const card = h(
    "div",
    { class: "month" },
    h("div", { class: "month-names", role: "row" }, ...Array.from({ length: 7 }, (_, i) => h("div", { role: "columnheader" }, dayShort(i)))),
    body,
  );
  // A crowded day shows as many chips as its tile is tall enough for, and counts the rest.
  new ResizeObserver(() => tiles.forEach(fitTile)).observe(body);
  return card;
}

function chip(app: App, lesson: Lesson, date: string, index: number): HTMLElement {
  const status = app.statusOf(lesson, date);
  const done = status === "opened" || status === "manual" || status === "skipped";
  const mark = status ? MARKS[status] : undefined;
  const element = h(
    "div",
    {
      class: done ? "mchip done" : "mchip",
      "data-tip": `${lesson.subject}\n${lesson.start}–${lesson.end}${teacherName(lesson) ? `\n${teacherName(lesson)}` : ""}`,
    },
    style(h("span", { class: "bar" }), { background: status === "missed" ? css("--danger") : subjectColor(lesson.subject) }),
    h("span", { class: "text" }, `${lesson.start} ${mark ? `${mark} ` : ""}${lesson.subject}`),
  );
  if (!done && status !== "missed") {
    const amount = Number(css("--subject-tint")) || 0.16;
    element.style.setProperty("--chip-fill", tint(css("--sunken"), subjectColor(lesson.subject), amount));
  }
  element.style.setProperty("view-transition-name", morphName(app, lesson, date, index));
  const ref: ClassRef = { lesson, date };
  draggable(element, ref, () => app.openDay(date), (target) => void app.dropClass(ref, target.date, lesson.start));
  element.addEventListener("pointerdown", (event) => event.stopPropagation());
  element.addEventListener("click", (event) => event.stopPropagation());
  return element;
}

function fitTile(tile: HTMLElement): void {
  const chips = [...tile.querySelectorAll<HTMLElement>(".mchip")];
  const more = tile.querySelector<HTMLElement>(".month-more");
  if (!more) return;
  const total = Number(tile.dataset.total ?? 0);
  chips.forEach((c) => (c.hidden = false));
  more.hidden = true;
  const top = tile.querySelector<HTMLElement>(".top");
  const room = tile.clientHeight - 12 - (top?.offsetHeight ?? 28) - 2;
  const chipHeight = (chips[0]?.offsetHeight ?? 18) + 2;
  let shown = chips.length;
  if (chips.length * chipHeight > room || total > chips.length) {
    more.textContent = "ще 0";
    more.hidden = false;
    shown = Math.max(0, Math.min(chips.length, Math.floor((room - more.offsetHeight - 2) / chipHeight)));
  }
  chips.forEach((c, i) => (c.hidden = i >= shown));
  const hidden = total - shown;
  more.textContent = t("month_more", { count: String(hidden) });
  more.hidden = hidden <= 0;
}
