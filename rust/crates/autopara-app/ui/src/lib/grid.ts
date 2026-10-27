// The week grid's geometry (PORT_SPEC §E.4, §E.5): one hour is a fixed number of pixels, so a
// class's place and height are its times, scaled.

export const FIRST_HOUR = 8;
export const LAST_HOUR = 18;
export const HOURS = Array.from({ length: LAST_HOUR - FIRST_HOUR + 1 }, (_, i) => FIRST_HOUR + i);
export const HOUR_HEIGHT = 66;
export const GUTTER_WIDTH = 74;
export const COLUMN_MIN_WIDTH = 150;
export const HEADER_HEIGHT = 72;
/** Anything shorter is still drawn this tall, so it stays readable and clickable. */
export const MIN_CARD_MINUTES = 26;

const DAY_START = FIRST_HOUR * 60;
const DAY_END = (LAST_HOUR + 1) * 60;

export function minutesToPixels(minutes: number): number {
  return Math.round((minutes * HOUR_HEIGHT) / 60);
}

export interface Span {
  /** 0-based hour row the class starts in. */
  row: number;
  /** How many hour rows it touches. */
  span: number;
  /** Minutes of the first row before it starts, and of the last row after it ends. */
  top: number;
  bottom: number;
}

/** Where a class sits (Python `WeekGrid.span_for`), clamped to the visible day. */
export function spanFor(start: number, end: number): Span {
  const from = Math.max(DAY_START, Math.min(start, DAY_END - 1));
  const to = Math.min(DAY_END, Math.max(end, from + MIN_CARD_MINUTES));
  const firstHour = Math.floor((from - DAY_START) / 60);
  const lastHour = Math.floor((to - DAY_START - 1) / 60);
  const top = from - DAY_START - firstHour * 60;
  const bottom = (lastHour + 1) * 60 - (to - DAY_START);
  return { row: firstHour, span: lastHour - firstHour + 1, top, bottom: Math.max(bottom, 1) };
}

/** A card's box inside its day column, in pixels from the top of the first hour row. */
export function cardBox(start: number, end: number): { top: number; height: number } {
  const { row, span, top, bottom } = spanFor(start, end);
  const topPx = minutesToPixels(top);
  return { top: row * HOUR_HEIGHT + topPx, height: span * HOUR_HEIGHT - topPx - minutesToPixels(bottom) };
}

/** Which lines a card shows (Python `ClassCard.LAYOUTS`), most to least. */
export interface CardLayout {
  subjectLines: number;
  teacher: boolean;
  time: boolean;
  detail: boolean;
}

const LAYOUTS: [number, boolean, boolean, boolean][] = [
  [3, true, true, true],
  [3, true, true, false],
  [2, true, true, false],
  [2, true, false, false],
  [2, false, true, false],
  [2, false, false, false],
  [1, true, true, false],
  [1, true, false, false],
  [1, false, true, false],
  [1, false, false, false],
];

export const CARD_PADDING_V = 4;
export const CARD_SPACING = 2;

export interface LineHeights {
  chips: number;
  subject: number;
  teacher: number;
  time: number;
}

/** The richest layout that fits `height` px; the barest one if none does. The time goes first,
 * since the card's place on the grid already says it. */
export function chooseLayout(height: number, lines: LineHeights): CardLayout {
  for (const [subjectLines, teacher, time, detail] of LAYOUTS) {
    const rows = 2 + Number(teacher) + Number(time) + Number(detail);
    const needed =
      CARD_PADDING_V * 2 +
      lines.chips +
      subjectLines * lines.subject +
      (teacher ? lines.teacher : 0) +
      (time ? lines.time : 0) +
      (detail ? lines.teacher : 0) +
      (rows - 1) * CARD_SPACING;
    if (needed <= height) return { subjectLines, teacher, time, detail };
  }
  return { subjectLines: 1, teacher: false, time: false, detail: false };
}

/** Where the "now" line goes, in px below the first hour row; null outside the grid. */
export function nowOffset(minutes: number): number | null {
  if (minutes < DAY_START || minutes >= DAY_END) return null;
  return minutesToPixels(minutes - DAY_START);
}
