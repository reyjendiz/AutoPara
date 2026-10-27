// Creating or changing a class (Python `ui/edit_dialog.py`). A class is a one-off, a weekly series
// with an end, or a weekly row; the form shows only the fields the kind needs and says in words
// what will happen. An imported class stays a weekly row.
import type { App } from "../../app";
import type { Lesson } from "../../bindings/Lesson";
import type { Recurrence } from "../../bindings/Recurrence";
import { h } from "../../dom";
import { t, type MessageKey } from "../../i18n";
import {
  type IsoDate, addDays, addMinutes, dayAccusative, dayName, lengthText, longDate, timesWord, toMinutes, weekday,
} from "../../lib/calendar";
import { inform, modal } from "../popups";

const DEFAULT_SERIES_DAYS = 28;

/** The next date (today included) that falls on `day`. */
function nextWeekday(day: number, today: IsoDate): IsoDate {
  return addDays(today, (day - weekday(today) + 7) % 7);
}

export function openEditor(app: App, lesson: Lesson | null, date: IsoDate, startAt?: string): Promise<boolean | null> {
  const title = lesson ? t("edit_title") : t("edit_new_title");
  return modal<boolean>((finish) => {
    const field = (attributes: Record<string, string>) => h("input", { class: "input", ...attributes });
    const subject = field({ placeholder: t("edit_subject_placeholder"), "aria-label": t("edit_subject") });
    const teacher = field({ placeholder: t("edit_teacher_placeholder"), "aria-label": t("edit_teacher") });
    const repeat = h(
      "select",
      { class: "select", "aria-label": t("edit_repeat"), onchange: () => refresh() },
      ...(["once", "weekly_until", "weekly"] as const).map((kind) =>
        h("option", { value: kind }, t(({ once: "edit_repeat_once", weekly_until: "edit_repeat_until", weekly: "edit_repeat_weekly" } as Record<string, MessageKey>)[kind] ?? "edit_repeat_once")),
      ),
    );
    const onDate = field({ type: "date", "aria-label": t("edit_date") });
    const until = field({ type: "date", "aria-label": t("edit_until") });
    const day = h("select", { class: "select", "aria-label": t("edit_day"), onchange: () => refresh() },
      ...Array.from({ length: 7 }, (_, i) => h("option", { value: String(i) }, dayName(i))));
    const start = field({ type: "time", "aria-label": t("edit_time") });
    const end = field({ type: "time", "aria-label": t("edit_time") });
    const url = field({ type: "url", placeholder: t("edit_link_placeholder"), "aria-label": t("edit_link") });
    const repeatHint = h("p", { class: "hint", role: "status" });
    const lengthHint = h("p", { class: "hint", role: "status" });
    const linkHint = h("p", { class: "hint", role: "status" });

    // The form: label, field. Rows that the kind does not need are hidden.
    const row = (key: MessageKey, ...fields: HTMLElement[]) => {
      const label = h("label", {}, t(key));
      const holder = fields.length === 1 ? (fields[0] as HTMLElement) : h("div", { class: "row" }, ...fields);
      return { label, holder, show: (on: boolean) => void (label.hidden = holder.hidden = !on) };
    };
    const rows = {
      subject: row("edit_subject", subject),
      teacher: row("edit_teacher", teacher),
      repeat: row("edit_repeat", repeat),
      date: row("edit_date", onDate),
      until: row("edit_until", until),
      day: row("edit_day", day),
      time: row("edit_time", start, h("span", { class: "hint" }, "–"), end),
      url: row("edit_link", url),
    };

    // Values: an existing class, or a new one-off on the date (and hour) it was asked for.
    const today = app.today();
    if (lesson) {
      subject.value = lesson.subject;
      teacher.value = lesson.teacher.replace(/^\(|\)$/g, "");
      url.value = lesson.url ?? "";
      start.value = lesson.start;
      end.value = lesson.end;
      day.value = String(lesson.weekday);
      const r = lesson.recurrence;
      const first = r.kind === "once" ? r.date : r.kind === "weekly_until" ? r.from : nextWeekday(lesson.weekday, today);
      onDate.value = first;
      until.value = r.kind === "weekly_until" ? r.until : addDays(first, DEFAULT_SERIES_DAYS);
      repeat.value = r.kind;
      rows.repeat.show(lesson.is_manual);
    } else {
      onDate.value = date;
      until.value = addDays(date, DEFAULT_SERIES_DAYS);
      day.value = String(weekday(date));
      repeat.value = "once";
      start.value = startAt ?? "08:00";
      end.value = app.defaultEnd(start.value);
    }

    let endTouched = Boolean(lesson);
    end.addEventListener("input", () => {
      endTouched = true;
      refresh();
    });
    start.addEventListener("input", () => {
      // The end follows the start until the user moves it themselves.
      if (!endTouched || end.value <= start.value) end.value = addMinutes(start.value, app.settings.class_duration_minutes);
      refresh();
    });
    onDate.addEventListener("input", () => {
      until.min = onDate.value;
      refresh();
    });
    until.addEventListener("input", refresh);
    url.addEventListener("input", refresh);

    function refresh(): void {
      const kind = repeat.value;
      rows.date.show(kind !== "weekly");
      rows.until.show(kind === "weekly_until");
      rows.day.show(kind === "weekly");
      until.min = onDate.value;
      if (kind === "once") {
        repeatHint.textContent = t("edit_hint_once", { day: dayName(weekday(onDate.value)), date: longDate(onDate.value) });
      } else if (kind === "weekly_until") {
        const count = Math.max(Math.floor((Date.parse(until.value) - Date.parse(onDate.value)) / 864e5 / 7) + 1, 1);
        repeatHint.textContent = t("edit_hint_until", {
          day: dayAccusative(weekday(onDate.value)),
          first: longDate(onDate.value),
          last: longDate(until.value),
          count: String(count),
          times: timesWord(count),
        });
      } else {
        repeatHint.textContent = t("edit_hint_weekly", { day: dayAccusative(Number(day.value)) });
      }
      const length = toMinutes(end.value) - toMinutes(start.value);
      lengthHint.textContent = length > 0 ? t("edit_duration", { length: lengthText(length) }) : t("edit_end_before_start");
      const link = url.value.trim();
      if (!link) linkHint.textContent = t("edit_no_link");
      else if (!link.toLowerCase().startsWith("https://")) linkHint.textContent = t("edit_https_only");
      else {
        const lower = link.toLowerCase();
        const provider = lower.includes("meet.google.com") ? "Google Meet" : lower.includes("zoom.us") || lower.includes("zoom.com") ? "Zoom" : t("edit_unknown_service");
        linkHint.textContent = t("edit_recognised", { provider });
      }
    }

    const save = async () => {
      const kind = repeat.value;
      const recurrence: Recurrence =
        kind === "weekly" ? { kind: "weekly" } : kind === "weekly_until" ? { kind: "weekly_until", from: onDate.value, until: until.value } : { kind: "once", date: onDate.value };
      try {
        await app.backend.saveLesson({
          id: lesson?.id ?? null,
          subject: subject.value,
          teacher: teacher.value,
          url: url.value.trim() || null,
          start: start.value,
          end: end.value,
          weekday: kind === "weekly" ? Number(day.value) : weekday(onDate.value),
          recurrence,
        });
        finish(true);
      } catch (error) {
        const code = String(error);
        const known = ["subject_required", "invalid_url", "invalid_time", "invalid_date"].includes(code);
        if (known) await inform(t(`error_${code}_title` as MessageKey), t(`error_${code}` as MessageKey));
        else await inform(t("error_generic_title"), code);
      }
    };

    refresh();
    const form = h("div", { class: "form" });
    for (const r of Object.values(rows)) form.append(r.label, r.holder);
    queueMicrotask(() => subject.focus());
    return h(
      "form",
      { class: "dialog edit", onsubmit: (event) => (event.preventDefault(), void save()) },
      h("h2", {}, lesson ? t("edit_title") : t("edit_create")),
      form,
      repeatHint,
      lengthHint,
      linkHint,
      h("div", { class: "footer" },
        h("button", { class: "btn plain", type: "button", onclick: () => finish(false) }, t("cancel")),
        h("button", { class: "btn primary", type: "submit" }, t("save"))),
    );
  }, title);
}
