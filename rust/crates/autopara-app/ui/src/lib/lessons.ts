// What a lesson does on a date, searching and filtering, and the "now / next" pill. Pure, so the
// week, the day, the month and the counts can never disagree (Python `filters.py`, `nextup.py`).
import type { Lesson } from "../bindings/Lesson";
import { t } from "../i18n";
import { type IsoDate, toMinutes, weekday, addDays } from "./calendar";

/** Whether `lesson` happens on `date` — the same rule as the Rust `Lesson::occurs_on`. */
export function occursOn(lesson: Lesson, date: IsoDate): boolean {
  if (weekday(date) !== lesson.weekday || lesson.skip_dates.includes(date)) return false;
  const r = lesson.recurrence;
  if (r.kind === "weekly") return true;
  if (r.kind === "once") return date === r.date;
  return r.from <= date && date <= (r.until > r.from ? r.until : r.from);
}

/** A weekly row, or a series spanning more than one date. */
export function repeats(lesson: Lesson): boolean {
  const r = lesson.recurrence;
  return r.kind === "weekly" || (r.kind === "weekly_until" && r.until > r.from);
}

/** The teacher as shown: the document writes it in parentheses. */
export function teacherName(lesson: Lesson): string {
  return lesson.teacher.replace(/^\(|\)$/g, "").trim();
}

const APOSTROPHES = /[`’ʼʹ‘´]/g;

/** Case- and apostrophe-insensitive comparison key, like the Python `normalize_text().casefold()`. */
export function fold(text: string): string {
  return text
    .normalize("NFC")
    .replace(APOSTROPHES, "'")
    .replace(/\s+/g, " ")
    .trim()
    .toLowerCase()
    .replace(/ß/g, "ss")
    .replace(/ς/g, "σ");
}

export interface LessonFilter {
  text: string;
  /** An exact subject; empty means all. */
  subject: string;
}

export const isActive = (filter: LessonFilter) => filter.text.trim() !== "" || filter.subject !== "";

/** Every word typed must be found in the subject or the teacher, in any order. */
export function matches(lesson: Lesson, filter: LessonFilter): boolean {
  if (filter.subject && fold(lesson.subject) !== fold(filter.subject)) return false;
  const words = fold(filter.text).split(" ").filter(Boolean);
  const haystack = `${fold(lesson.subject)} ${fold(lesson.teacher)}`;
  return words.every((word) => haystack.includes(word));
}

const ALPHABET = "абвгґдеєжзиіїйклмнопрстуфхцчшщьюяabcdefghijklmnopqrstuvwxyz";

/** Ukrainian alphabetical order (code points would put "і" before "а"). */
export function compareUkrainian(a: string, b: string): number {
  const key = (text: string) =>
    [...fold(text)].map((char) => {
      const index = ALPHABET.indexOf(char);
      return index >= 0 ? index : ALPHABET.length + (char.codePointAt(0) ?? 0);
    });
  const [x, y] = [key(a), key(b)];
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    if (x[i] !== y[i]) return (x[i] ?? 0) - (y[i] ?? 0);
  }
  return x.length - y.length;
}

/** Every distinct subject, once, alphabetically — what the subject drop-down offers. */
export function subjects(lessons: Lesson[]): string[] {
  const seen = new Map<string, string>();
  for (const lesson of lessons) {
    const key = fold(lesson.subject);
    if (!seen.has(key)) seen.set(key, lesson.subject.normalize("NFC").replace(/\s+/g, " ").trim());
  }
  return [...seen.values()].sort(compareUkrainian);
}

const SEARCH_HORIZON_DAYS = 400;

/** The nearest date after (`direction` 1) or before (-1) `start` with a matching class. */
export function findMatch(lessons: Lesson[], filter: LessonFilter, start: IsoDate, direction: 1 | -1): IsoDate | null {
  const candidates = lessons.filter((lesson) => matches(lesson, filter));
  if (!candidates.length) return null;
  let day = start;
  for (let i = 0; i < SEARCH_HORIZON_DAYS; i++) {
    day = addDays(day, direction);
    if (candidates.some((lesson) => occursOn(lesson, day))) return day;
  }
  return null;
}

// ------------------------------------------------------------------ now / next

const COUNTDOWN_LIMIT_MINUTES = 90;

export interface NextUp {
  lesson: Lesson;
  ongoing: boolean;
  minutes: number;
}

/** The class that is on now, else the next one today; skipped ones do not count. */
export function nextUp(lessons: Lesson[], today: IsoDate, nowMinutes: number, settled: Set<number>): NextUp | null {
  const candidates = lessons.filter(
    (lesson) => occursOn(lesson, today) && !settled.has(lesson.id) && toMinutes(lesson.end) > nowMinutes,
  );
  if (!candidates.length) return null;
  const lesson = candidates.reduce((a, b) => (toMinutes(b.start) < toMinutes(a.start) ? b : a));
  const [start, end] = [toMinutes(lesson.start), toMinutes(lesson.end)];
  return start <= nowMinutes
    ? { lesson, ongoing: true, minutes: Math.max(1, end - nowMinutes) }
    : { lesson, ongoing: false, minutes: Math.max(1, start - nowMinutes) };
}

export function nextUpLabel(item: NextUp): string {
  const vars = { subject: item.lesson.subject, minutes: String(item.minutes), start: item.lesson.start };
  if (item.ongoing) return t("nextup_now", vars);
  return item.minutes <= COUNTDOWN_LIMIT_MINUTES ? t("nextup_in", vars) : t("nextup_at", vars);
}
