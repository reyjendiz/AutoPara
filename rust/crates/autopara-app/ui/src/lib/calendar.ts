// Dates, times and the Ukrainian words around them. Pure functions; no clock is read here.
import { t, type MessageKey } from "../i18n";

/** A calendar date as `YYYY-MM-DD`, the way the Rust side sends it. */
export type IsoDate = string;

/** Parse an ISO date into a local Date at noon (noon keeps DST shifts from changing the day). */
export function parseIso(iso: IsoDate): Date {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1, 12);
}

export function toIso(date: Date): IsoDate {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

export function addDays(iso: IsoDate, days: number): IsoDate {
  const date = parseIso(iso);
  date.setDate(date.getDate() + days);
  return toIso(date);
}

/** 0 = Monday … 6 = Sunday, as the timetable counts. */
export function weekday(iso: IsoDate): number {
  return (parseIso(iso).getDay() + 6) % 7;
}

export function weekStart(iso: IsoDate): IsoDate {
  return addDays(iso, -weekday(iso));
}

export function firstOfMonth(iso: IsoDate): IsoDate {
  return `${iso.slice(0, 8)}01`;
}

function daysInMonth(year: number, monthIndex: number): number {
  return new Date(year, monthIndex + 1, 0).getDate();
}

/** `iso` moved by whole months, clamped to a shorter month's last day (31 Jan → 28 Feb). */
export function shiftMonth(iso: IsoDate, months: number): IsoDate {
  const date = parseIso(iso);
  const index = date.getFullYear() * 12 + date.getMonth() + months;
  const year = Math.floor(index / 12);
  const month = index - year * 12;
  const day = Math.min(date.getDate(), daysInMonth(year, month));
  return toIso(new Date(year, month, day, 12));
}

/** The 42 dates a month view shows, from the Monday on or before the 1st. */
export function monthGrid(iso: IsoDate): IsoDate[] {
  const start = weekStart(firstOfMonth(iso));
  return Array.from({ length: 42 }, (_, i) => addDays(start, i));
}

export function dayName(index: number): string {
  return t(`day_${index}` as MessageKey);
}

export function dayShort(index: number): string {
  return t(`day_short_${index}` as MessageKey);
}

export function dayAccusative(index: number): string {
  return t(`day_acc_${index}` as MessageKey);
}

export function monthName(index: number): string {
  return t(`month_${index}` as MessageKey);
}

export function monthGenitive(index: number): string {
  return t(`month_gen_${index}` as MessageKey);
}

/** `7 жовтня` */
export function longDate(iso: IsoDate): string {
  const date = parseIso(iso);
  return `${date.getDate()} ${monthGenitive(date.getMonth())}`;
}

/** The navigation bar's title for a week, as `[period, quiet year]` (Python `week_parts`). */
export function weekTitle(monday: IsoDate): [string, string] {
  const first = parseIso(monday);
  const last = parseIso(addDays(monday, 6));
  const [m1, m2] = [monthGenitive(first.getMonth()), monthGenitive(last.getMonth())];
  if (first.getFullYear() !== last.getFullYear()) {
    return [`${first.getDate()} ${m1} ${first.getFullYear()} – ${last.getDate()} ${m2}`, String(last.getFullYear())];
  }
  if (first.getMonth() !== last.getMonth()) {
    return [`${first.getDate()} ${m1} – ${last.getDate()} ${m2}`, String(last.getFullYear())];
  }
  return [`${first.getDate()}–${last.getDate()} ${m2}`, String(last.getFullYear())];
}

export function dayTitle(iso: IsoDate): [string, string] {
  const date = parseIso(iso);
  return [`${dayName(weekday(iso))}, ${longDate(iso)}`, String(date.getFullYear())];
}

export function monthTitle(iso: IsoDate): [string, string] {
  const date = parseIso(iso);
  return [monthName(date.getMonth()), String(date.getFullYear())];
}

// ------------------------------------------------------------------- times

/** `HH:MM` → minutes since midnight. */
export function toMinutes(hhmm: string): number {
  const [h, m] = hhmm.split(":").map(Number);
  return (h ?? 0) * 60 + (m ?? 0);
}

export function fromMinutes(minutes: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(minutes / 60))}:${pad(minutes % 60)}`;
}

/** `hhmm + minutes`, clamped at 23:59 (Python `add_minutes`). */
export function addMinutes(hhmm: string, minutes: number): string {
  return fromMinutes(Math.min(toMinutes(hhmm) + minutes, 24 * 60 - 1));
}

/** `1 год 20 хв`, `45 хв` */
export function lengthText(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  const parts: string[] = [];
  if (hours) parts.push(t("edit_hours", { count: String(hours) }));
  if (rest) parts.push(t("edit_minutes", { count: String(rest) }));
  return parts.join(" ");
}

// ------------------------------------------------------------------ plurals

function plural(count: number, one: MessageKey | null, few: MessageKey, many: MessageKey): string {
  const lastTwo = count % 100;
  const last = count % 10;
  if (one && last === 1 && lastTwo !== 11) return t(one);
  if (last >= 2 && last <= 4 && (lastTwo < 12 || lastTwo > 14)) return t(few);
  return t(many);
}

/** `1 пара`, `2 пари`, `5 пар` */
export const pairsWord = (n: number) => plural(n, "plural_pair_one", "plural_pair_few", "plural_pair_many");
/** `2 групи`, `5 груп` (there is never one group in a shared class) */
export const groupsWord = (n: number) => plural(n, null, "plural_group_few", "plural_group_many");
/** `1 раз`, `2 рази`, `5 разів` */
export const timesWord = (n: number) => plural(n, "plural_time_one", "plural_time_few", "plural_time_many");
